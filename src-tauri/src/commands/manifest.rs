//! 环境清单导出/导入：把「Agent 注册表 + 技能分配 + 收藏标签」导出为
//! 单个 JSON 清单，可在新机器导入恢复整套分配关系。
//! 与 Git 备份互补：Git 管技能文件历史，清单管"配置即文件"的迁移。

use crate::models::*;
use std::fs;
use std::path::Path;

/// 当前清单格式版本
const MANIFEST_VERSION: u32 = 1;
/// 导出文件名前缀（latest_manifest_path 依赖此约定）
const FILE_PREFIX: &str = "agentskillhub-manifest-";

/// Unix 时间戳 → "YYYYMMDD-HHmmss"（UTC，仅用于文件名，零依赖不引 chrono）
fn ts_to_filename(ts: u64) -> String {
    let days = ts / 86400;
    let secs_of_day = ts % 86400;
    // Howard Hinnant civil-from-days 算法
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    let (h, mi, s) = (
        secs_of_day / 3600,
        secs_of_day % 3600 / 60,
        secs_of_day % 60,
    );
    format!("{y:04}{m:02}{d:02}-{h:02}{mi:02}{s:02}")
}

/// 导出清单到技能库根目录，返回落盘绝对路径
pub fn write_manifest(cfg: &AppConfig, app_version: &str) -> Result<String, String> {
    let manifest = Manifest {
        manifest_version: MANIFEST_VERSION,
        exported_ts: crate::commands::now_ts(),
        app_version: app_version.to_string(),
        skills_root: cfg.skills_root.clone(),
        agents: cfg.agents.clone(),
        agent_configs: cfg.agent_configs.clone(),
        skill_meta: cfg.skill_meta.clone(),
    };
    if cfg.skills_root.trim().is_empty() {
        return Err("技能库根目录未设置".into());
    }
    let file = Path::new(&cfg.skills_root).join(format!(
        "{FILE_PREFIX}{}.json",
        ts_to_filename(manifest.exported_ts)
    ));
    let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    fs::write(&file, json).map_err(|e| format!("写入清单失败: {e}"))?;
    Ok(file.to_string_lossy().to_string())
}

/// 导入清单：
/// - agents 按 id 只增不改（skills_dir 是机器相关的本地路径）
/// - agent_configs 按 agent id 整体覆盖（清单是分配关系快照，覆盖幂等且能还原"取消分配"）
/// - skill_meta 按 key 并入覆盖
/// - skills_root 不覆盖（机器相关，仅作信息记录）
pub fn import_manifest(
    cfg: &mut AppConfig,
    path: &str,
    allowed: &std::collections::HashSet<String>,
) -> Result<ManifestImportResult, String> {
    let content = fs::read_to_string(path).map_err(|e| format!("读取清单失败: {e}"))?;
    // 先校验版本，给用户明确的兼容性提示
    let v: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("清单不是合法 JSON: {e}"))?;
    let ver = v
        .get("manifest_version")
        .and_then(|x| x.as_u64())
        .unwrap_or(0);
    if ver != MANIFEST_VERSION as u64 {
        return Err(format!(
            "清单版本不兼容（v{ver}，当前支持 v{MANIFEST_VERSION}），请升级应用后再导入"
        ));
    }
    let mut manifest: Manifest =
        serde_json::from_value(v).map_err(|e| format!("清单结构不合法: {e}"))?;
    crate::commands::scan::normalize_skill_references(
        &cfg.skills_root, &mut manifest.agent_configs, &mut manifest.skill_meta,
    );

    // agents 只增不改
    for a in &manifest.agents {
        if !cfg.agents.iter().any(|x| x.id == a.id) {
            cfg.agents.push(a.clone());
            cfg.agent_preferences.entry(a.id.clone()).or_default();
            cfg.agent_configs.entry(a.id.clone()).or_default();
        }
    }
    // 分配关系整体覆盖（仅覆盖清单里出现过的 agent）
    let mut overwritten = 0usize;
    for (id, ac) in &manifest.agent_configs {
        if !allowed.contains(id) || !crate::commands::agent_policy::enabled(cfg, id) {
            continue;
        }
        if cfg
            .agent_configs
            .get(id)
            .map(|c| c.enabled_skills != ac.enabled_skills)
            .unwrap_or(true)
        {
            overwritten += 1;
        }
        cfg.agent_configs.insert(id.clone(), ac.clone());
    }
    // 收藏标签并入
    for (k, m) in &manifest.skill_meta {
        cfg.skill_meta.insert(k.clone(), m.clone());
    }

    // 统计指向缺失技能的分配条数（导入后前端显示为"已失效"）
    let missing = cfg
        .agent_configs
        .values()
        .flat_map(|c| c.enabled_skills.iter())
        .filter(|k| crate::commands::scan::resolve_skill_target(&cfg.skills_root, k).is_none())
        .count() as u32;

    Ok(ManifestImportResult {
        ok: true,
        message: format!(
            "导入完成：覆盖 {overwritten} 个 Agent 的分配（共 {} 个 Agent），收藏标签 {} 条",
            manifest.agent_configs.len(),
            manifest.skill_meta.len()
        ),
        missing_skills: missing,
    })
}

/// 找技能库根目录里最新的清单文件（按文件名时间戳倒序）
pub fn latest_manifest_path(skills_root: &str) -> Option<String> {
    let rd = fs::read_dir(skills_root).ok()?;
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            // 先克隆一份给 borrow，strip_prefix 不影响 ownership
            let filename = name.clone();
            let rest = filename.strip_prefix(FILE_PREFIX)?;
            rest.ends_with(".json").then_some((name, e.path()))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, p)| p.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp(tag: &str) -> PathBuf {
        let d = crate::commands::test_scratch(&format!("manifest_{tag}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn sample_cfg(root: &str) -> AppConfig {
        let mut cfg = AppConfig { skills_root: root.into(), token_budget: 8000, ..AppConfig::default() };
        cfg.agents.push(AgentDef {
            id: "claude".into(),
            name: "Claude Code".into(),
            skills_dir: r"C:\x\skills".into(),
            builtin: true,
        });
        let ac = AgentConfig { enabled_skills: vec!["a".into(), "b".into()] };
        cfg.agent_configs.insert("claude".into(), ac);
        let meta = SkillUserMeta { starred: true, ..SkillUserMeta::default() };
        cfg.skill_meta.insert("a".into(), meta);
        cfg
    }

    #[test]
    fn roundtrip_and_overwrite_semantics() {
        let dir = tmp("rt");
        let root = dir.to_string_lossy().to_string();
        let cfg = sample_cfg(&root);
        let path = write_manifest(&cfg, "0.2.1").unwrap();
        assert!(Path::new(&path).is_file(), "清单应落盘到技能库根目录");

        // 本地状态被改过：a 被取消、多了 c 和多余 agent
        let mut local = sample_cfg(&root);
        local
            .agent_configs
            .get_mut("claude")
            .unwrap()
            .enabled_skills = vec!["c".into()];
        local.agents.push(AgentDef {
            id: "extra".into(),
            name: "X".into(),
            skills_dir: r"C:\y".into(),
            builtin: false,
        });
        local.skill_meta.clear();

        let r =
            import_manifest(&mut local, &path, &["claude".into()].into_iter().collect()).unwrap();
        assert!(r.ok);
        // 覆盖语义：分配被还原为 a,b
        assert_eq!(
            local.agent_configs["claude"].enabled_skills,
            vec!["a".to_string(), "b".to_string()]
        );
        // 本地多余的 agent 不被删除（只增不改）
        assert!(local.agents.iter().any(|a| a.id == "extra"));
        // 收藏并入
        assert!(local.skill_meta["a"].starred);
        // skills_root 不被覆盖
        assert_eq!(local.skills_root, root);
        // 技能 a/b 在空库里不存在 → missing 统计
        assert_eq!(r.missing_skills, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn import_preserves_closed_agents_and_never_enables_new_tools() {
        let dir = tmp("closed");
        let cfg = sample_cfg(&dir.to_string_lossy());
        let path = write_manifest(&cfg, "0.2.1").unwrap();
        let mut local = cfg.clone();
        local
            .agent_configs
            .get_mut("claude")
            .unwrap()
            .enabled_skills = vec!["preserved".into()];
        local.agent_preferences.entry("claude".into()).or_default();
        import_manifest(&mut local, &path, &["claude".into()].into_iter().collect()).unwrap();
        assert_eq!(local.agent_configs["claude"].enabled_skills, ["preserved"]);
        local.agents.clear();
        local.agent_configs.clear();
        local.agent_preferences.clear();
        import_manifest(&mut local, &path, &Default::default()).unwrap();
        assert!(!crate::commands::agent_policy::enabled(&local, "claude"));
        assert!(local.agent_configs["claude"].enabled_skills.is_empty());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn import_old_manifest_maps_categorized_keys_and_metadata() {
        let dir = tmp("classified");
        let root = dir.to_string_lossy().to_string();
        let cfg = sample_cfg(&root);
        let file = write_manifest(&cfg, "1.1.1").unwrap();
        fs::create_dir_all(dir.join("开发/a")).unwrap();
        fs::write(dir.join("开发/a/SKILL.md"), "---\nname: a\n---").unwrap();
        fs::write(dir.join(".agentskillhub-skill-aliases.json"), r#"{"version":1,"aliases":{"a":"开发/a"}}"#).unwrap();
        let mut local = cfg;
        local.agent_configs.clear();
        local.skill_meta.clear();
        let result = import_manifest(&mut local, &file, &["claude".into()].into_iter().collect()).unwrap();
        assert_eq!(local.agent_configs["claude"].enabled_skills, ["开发/a", "b"]);
        assert!(local.skill_meta["开发/a"].starred);
        assert_eq!(result.missing_skills, 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_bad_version() {
        let dir = tmp("ver");
        let p = dir.join("m.json");
        fs::write(&p, r#"{"manifest_version": 2}"#).unwrap();
        let mut cfg = sample_cfg(&dir.to_string_lossy());
        let err = import_manifest(&mut cfg, &p.to_string_lossy(), &Default::default()).unwrap_err();
        assert!(err.contains("版本不兼容"), "got: {err}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_manifest_picks_newest() {
        let dir = tmp("latest");
        fs::write(dir.join(format!("{FILE_PREFIX}20260101-000000.json")), "{}").unwrap();
        fs::write(dir.join(format!("{FILE_PREFIX}20260901-120000.json")), "{}").unwrap();
        fs::write(dir.join("other.json"), "{}").unwrap();
        let got = latest_manifest_path(&dir.to_string_lossy()).unwrap();
        assert!(got.contains("20260901-120000"), "got: {got}");
        assert_eq!(
            latest_manifest_path(&dir.join("nonexist").to_string_lossy()),
            None
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn ts_filename_format() {
        // 2026-09-23 00:00:00 UTC = 1789300800... 只验证格式
        let s = ts_to_filename(0);
        assert_eq!(s, "19700101-000000");
    }
}
