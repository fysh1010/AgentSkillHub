use crate::models::*;
use std::fs;
use std::path::{Path, PathBuf};

/// 判断路径是否为 junction / 符号链接
pub fn is_junction(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(md) => md.file_type().is_symlink(),
        Err(_) => false,
    }
}

/// 解析 junction 目标
pub fn junction_target(path: &Path) -> Option<PathBuf> {
    fs::read_link(path).ok()
}

/// 创建 junction（mklink /J，无需管理员权限）
pub fn create_junction(link: &Path, target: &Path) -> Result<(), String> {
    if let Some(parent) = link.parent() {
        if !parent.is_dir() {
            fs::create_dir_all(parent).map_err(|e| format!("创建父目录失败: {e}"))?;
        }
    }
    if link.exists() || is_junction(link) {
        return Err(format!("目标路径已存在: {}", link.display()));
    }
    let out = crate::commands::hidden_command("cmd")
        .args(["/c", "mklink", "/J", &link.to_string_lossy(), &target.to_string_lossy()])
        .output()
        .map_err(|e| format!("调用 mklink 失败: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// 删除 junction（只删链接本身，不碰目标）
pub fn remove_junction(link: &Path) -> Result<(), String> {
    if is_junction(link) {
        fs::remove_dir(link).map_err(|e| format!("删除链接失败 {}: {e}", link.display()))?;
        Ok(())
    } else {
        Err(format!("不是链接，拒绝删除: {}", link.display()))
    }
}

/// Agent skills 目录的实际扫描结果
struct AgentLinks {
    /// skills_dir 本身是否是整目录 junction（旧模式）
    parent_link: Option<PathBuf>,
    /// 第一层子目录列表：(名字, 类型 link/external, 目标)
    entries: Vec<(String, String, Option<PathBuf>)>,
}

/// 扫描 agent skills 目录的实际状态
fn scan_agent_links(skills_dir: &Path) -> AgentLinks {
    let mut parent_link = None;
    if is_junction(skills_dir) {
        parent_link = junction_target(skills_dir);
    }
    let mut entries = Vec::new();
    if let Ok(rd) = fs::read_dir(skills_dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            // 点开头的条目（.system 等）由各 Agent 工具自行管理，本工具一律不感知
            if name.starts_with('.') {
                continue;
            }
            let p = e.path();
            if is_junction(&p) {
                entries.push((name, "link".into(), junction_target(&p)));
            } else if p.is_dir() {
                entries.push((name, "external".into(), None));
            }
        }
    }
    AgentLinks { parent_link, entries }
}

/// 判断路径是否位于技能库根内
pub(crate) fn within_root(root: &str, p: &Path) -> bool {
    let root_p = Path::new(root);
    let p_norm = normalize(p);
    let root_norm = normalize(root_p);
    p_norm.starts_with(&root_norm)
}

pub(crate) fn normalize(p: &Path) -> PathBuf {
    let s = p.to_string_lossy().replace('/', "\\");
    PathBuf::from(s.trim_end_matches('\\'))
}

/// 计算期望状态 vs 实际状态的差异
pub fn compute_diff(agent: &AgentDef, cfg: &AppConfig) -> Vec<DiffItem> {
    let mut diff = Vec::new();
    let skills_dir = Path::new(&agent.skills_dir);
    let skills_root = &cfg.skills_root;
    let enabled: Vec<String> = cfg
        .agent_configs
        .get(&agent.id)
        .map(|c| c.enabled_skills.clone())
        .unwrap_or_default();
    let links = scan_agent_links(skills_dir);

    // 1. 整目录链接（旧 _common 模式）处理
    if let Some(parent_t) = &links.parent_link {
        if within_root(skills_root, parent_t) {
            diff.push(DiffItem {
                kind: "expand_parent_link".into(),
                skill_key: "_parent".into(),
                link_path: agent.skills_dir.clone(),
                current_target: Some(parent_t.to_string_lossy().to_string()),
                expected_target: None,
            });
        } else {
            diff.push(DiffItem {
                kind: "skip".into(),
                skill_key: "_parent".into(),
                link_path: agent.skills_dir.clone(),
                current_target: Some(parent_t.to_string_lossy().to_string()),
                expected_target: None,
            });
        }
    }

    // 期望 key -> 目标路径
    let mut expected: Vec<(String, PathBuf)> = Vec::new();
    let mut name_taken: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for key in &enabled {
        if let Some(t) = crate::commands::scan::resolve_skill_target(skills_root, key) {
            let name = key.rsplit('/').next().unwrap_or(key).to_string();
            if let Some(prev) = name_taken.insert(name.clone(), key.clone()) {
                diff.push(DiffItem {
                    kind: "conflict".into(),
                    skill_key: key.clone(),
                    link_path: format!("{}\\{}", agent.skills_dir, name),
                    current_target: Some(format!("同名冲突: {} 与 {}", prev, key)),
                    expected_target: None,
                });
                continue;
            }
            expected.push((key.clone(), t));
        }
    }

    // 2. 期望要创建/保留的
    for (key, target) in &expected {
        let name = key.rsplit('/').next().unwrap_or(key).to_string();
        let link_path = skills_dir.join(&name);
        match links.entries.iter().find(|(n, _, _)| n == &name) {
            Some((_, k, Some(cur))) if k == "link" => {
                let cur_norm = normalize(cur);
                let tgt_norm = normalize(target);
                if cur_norm == tgt_norm {
                    diff.push(DiffItem {
                        kind: "keep".into(),
                        skill_key: key.clone(),
                        link_path: link_path.to_string_lossy().to_string(),
                        current_target: Some(cur.to_string_lossy().to_string()),
                        expected_target: Some(target.to_string_lossy().to_string()),
                    });
                } else {
                    diff.push(DiffItem {
                        kind: "to_replace".into(),
                        skill_key: key.clone(),
                        link_path: link_path.to_string_lossy().to_string(),
                        current_target: Some(cur.to_string_lossy().to_string()),
                        expected_target: Some(target.to_string_lossy().to_string()),
                    });
                }
            }
            Some((_, k, None)) if k == "link" => {
                diff.push(DiffItem {
                    kind: "to_create".into(),
                    skill_key: key.clone(),
                    link_path: link_path.to_string_lossy().to_string(),
                    current_target: None,
                    expected_target: Some(target.to_string_lossy().to_string()),
                });
            }
            Some((_, k, _)) if k == "external" => {
                diff.push(DiffItem {
                    kind: "external".into(),
                    skill_key: key.clone(),
                    link_path: link_path.to_string_lossy().to_string(),
                    current_target: Some("外部目录，占用同名".into()),
                    expected_target: Some(target.to_string_lossy().to_string()),
                });
            }
            _ => {
                diff.push(DiffItem {
                    kind: "to_create".into(),
                    skill_key: key.clone(),
                    link_path: link_path.to_string_lossy().to_string(),
                    current_target: None,
                    expected_target: Some(target.to_string_lossy().to_string()),
                });
            }
        }
    }

    // 3. 实际存在但不在期望中的（本工具管理的链接 → 移除；外部目录 → 跳过）
    for (name, kind, tgt) in &links.entries {
        if expected.iter().any(|(k, _)| k.rsplit('/').next().unwrap_or(k) == name) {
            continue;
        }
        if kind == "link" {
            if let Some(t) = tgt {
                if within_root(skills_root, t) {
                    diff.push(DiffItem {
                        kind: "to_remove".into(),
                        skill_key: name.clone(),
                        link_path: skills_dir.join(name).to_string_lossy().to_string(),
                        current_target: Some(t.to_string_lossy().to_string()),
                        expected_target: None,
                    });
                } else {
                    diff.push(DiffItem {
                        kind: "external".into(),
                        skill_key: name.clone(),
                        link_path: skills_dir.join(name).to_string_lossy().to_string(),
                        current_target: Some("指向技能库外，保留".into()),
                        expected_target: None,
                    });
                }
            } else {
                diff.push(DiffItem {
                    kind: "external".into(),
                    skill_key: name.clone(),
                    link_path: skills_dir.join(name).to_string_lossy().to_string(),
                    current_target: Some("无法解析目标，保留".into()),
                    expected_target: None,
                });
            }
        } else {
            diff.push(DiffItem {
                kind: "external".into(),
                skill_key: name.clone(),
                link_path: skills_dir.join(name).to_string_lossy().to_string(),
                current_target: Some("外部目录，不动".into()),
                expected_target: None,
            });
        }
    }

    diff
}

/// 执行部署（dry_run 时只计算并标记，不实际改动）
pub fn deploy(agent: &AgentDef, cfg: &AppConfig, dry_run: bool) -> DeployResult {
    let diff = compute_diff(agent, cfg);
    let mut actions = Vec::new();
    let mut errs = 0usize;

    for item in &diff {
        match item.kind.as_str() {
            "expand_parent_link" => {
                let link = Path::new(&item.link_path);
                let target = item.current_target.clone().unwrap_or_default();
                if dry_run {
                    actions.push(DeployAction {
                        kind: "remove_parent_link".into(),
                        skill_key: "_parent".into(),
                        link_path: item.link_path.clone(),
                        target: target.clone(),
                        ok: true,
                        message: "将整目录链接迁移为逐技能链接（干跑）".into(),
                    });
                } else {
                    match remove_junction(link) {
                        Ok(()) => {
                            let _ = fs::create_dir_all(link);
                            actions.push(DeployAction {
                                kind: "remove_parent_link".into(),
                                skill_key: "_parent".into(),
                                link_path: item.link_path.clone(),
                                target: target.clone(),
                                ok: true,
                                message: "整目录链接已迁移为真实目录".into(),
                            });
                        }
                        Err(e) => {
                            errs += 1;
                            actions.push(DeployAction {
                                kind: "remove_parent_link".into(),
                                skill_key: "_parent".into(),
                                link_path: item.link_path.clone(),
                                target,
                                ok: false,
                                message: e,
                            });
                        }
                    }
                }
            }
            "to_create" | "to_replace" => {
                let link = Path::new(&item.link_path);
                let target = item.expected_target.clone().unwrap_or_default();
                if item.kind == "to_replace" && !dry_run {
                    let _ = remove_junction(link);
                }
                if dry_run {
                    actions.push(DeployAction {
                        kind: "create_link".into(),
                        skill_key: item.skill_key.clone(),
                        link_path: item.link_path.clone(),
                        target,
                        ok: true,
                        message: "将创建链接（干跑）".into(),
                    });
                } else {
                    match create_junction(link, Path::new(&target)) {
                        Ok(()) => actions.push(DeployAction {
                            kind: "create_link".into(),
                            skill_key: item.skill_key.clone(),
                            link_path: item.link_path.clone(),
                            target,
                            ok: true,
                            message: "链接已创建".into(),
                        }),
                        Err(e) => {
                            errs += 1;
                            actions.push(DeployAction {
                                kind: "create_link".into(),
                                skill_key: item.skill_key.clone(),
                                link_path: item.link_path.clone(),
                                target,
                                ok: false,
                                message: e,
                            });
                        }
                    }
                }
            }
            "to_remove" => {
                let link = Path::new(&item.link_path);
                if dry_run {
                    actions.push(DeployAction {
                        kind: "remove_link".into(),
                        skill_key: item.skill_key.clone(),
                        link_path: item.link_path.clone(),
                        target: item.current_target.clone().unwrap_or_default(),
                        ok: true,
                        message: "将移除链接（干跑）".into(),
                    });
                } else {
                    match remove_junction(link) {
                        Ok(()) => actions.push(DeployAction {
                            kind: "remove_link".into(),
                            skill_key: item.skill_key.clone(),
                            link_path: item.link_path.clone(),
                            target: item.current_target.clone().unwrap_or_default(),
                            ok: true,
                            message: "链接已移除".into(),
                        }),
                        Err(e) => {
                            errs += 1;
                            actions.push(DeployAction {
                                kind: "remove_link".into(),
                                skill_key: item.skill_key.clone(),
                                link_path: item.link_path.clone(),
                                target: item.current_target.clone().unwrap_or_default(),
                                ok: false,
                                message: e,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let created = actions.iter().filter(|a| a.kind == "create_link" && a.ok).count();
    let removed = actions.iter().filter(|a| a.kind == "remove_link" && a.ok).count();
    let summary = if dry_run {
        format!("干跑完成：将创建 {} 个链接，移除 {} 个链接", created, removed)
    } else if errs == 0 {
        format!("部署完成：已创建 {} 个链接，移除 {} 个链接", created, removed)
    } else {
        format!("部署完成（{errs} 个失败）：已创建 {created} 个，移除 {removed} 个")
    };

    DeployResult { ok: errs == 0, actions, summary }
}

/// 回滚最近一次部署（反向执行其动作）
pub fn rollback_last(cfg: &AppConfig, agent_id: &str) -> DeployResult {
    let entry = cfg.audit.iter().rev().find(|a| a.agent_id == agent_id);
    let Some(entry) = entry else {
        return DeployResult {
            ok: false,
            actions: Vec::new(),
            summary: "没有可回滚的部署记录".into(),
        };
    };
    let mut actions = Vec::new();
    let mut errs = 0usize;
    for a in entry.actions.iter().rev() {
        match a.kind.as_str() {
            "create_link" => {
                let link = Path::new(&a.link_path);
                if a.ok && is_junction(link) {
                    match remove_junction(link) {
                        Ok(()) => actions.push(DeployAction {
                            kind: "remove_link".into(),
                            skill_key: a.skill_key.clone(),
                            link_path: a.link_path.clone(),
                            target: a.target.clone(),
                            ok: true,
                            message: "回滚：已移除链接".into(),
                        }),
                        Err(e) => {
                            errs += 1;
                            actions.push(DeployAction {
                                kind: "remove_link".into(),
                                skill_key: a.skill_key.clone(),
                                link_path: a.link_path.clone(),
                                target: a.target.clone(),
                                ok: false,
                                message: e,
                            });
                        }
                    }
                }
            }
            "remove_link" => {
                let target = Path::new(&a.target);
                if a.ok && !a.target.is_empty() {
                    match create_junction(Path::new(&a.link_path), target) {
                        Ok(()) => actions.push(DeployAction {
                            kind: "create_link".into(),
                            skill_key: a.skill_key.clone(),
                            link_path: a.link_path.clone(),
                            target: a.target.clone(),
                            ok: true,
                            message: "回滚：已恢复链接".into(),
                        }),
                        Err(e) => {
                            errs += 1;
                            actions.push(DeployAction {
                                kind: "create_link".into(),
                                skill_key: a.skill_key.clone(),
                                link_path: a.link_path.clone(),
                                target: a.target.clone(),
                                ok: false,
                                message: e,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }
    DeployResult {
        ok: errs == 0,
        actions,
        summary: if errs == 0 { "回滚完成".into() } else { format!("回滚完成（{errs} 个失败）") },
    }
}

/// 计算 Agent 状态（总览用）
pub fn agent_status(agent: &AgentDef, cfg: &AppConfig) -> AgentStatus {
    let skills_dir = Path::new(&agent.skills_dir);
    let dir_ok = skills_dir.is_dir();
    let enabled_count = cfg
        .agent_configs
        .get(&agent.id)
        .map(|c| c.enabled_skills.len())
        .unwrap_or(0);
    let links = scan_agent_links(skills_dir);
    let linked_count = links
        .entries
        .iter()
        .filter(|(_, k, t)| {
            k == "link"
                && t.as_ref()
                    .map(|tt| within_root(&cfg.skills_root, tt))
                    .unwrap_or(false)
        })
        .count();
    let external_count = links
        .entries
        .iter()
        .filter(|(_, k, _)| k == "external")
        .count();
    // skills 本身是整目录链接时：
    // - 指向技能库内 → 旧 _common 模式，待迁移
    // - 指向技能库外（用户自行把目录重定向到其他盘）→ 本工具不触碰，也不算漂移
    let parent_internal = links
        .parent_link
        .as_ref()
        .map(|pt| within_root(&cfg.skills_root, pt))
        .unwrap_or(false);
    // 整目录链接下可见的技能子目录数
    let parent_visible = match &links.parent_link {
        Some(pt) if parent_internal => {
            if let Ok(rd) = fs::read_dir(pt) {
                rd.flatten()
                    .filter(|e| e.path().join("SKILL.md").is_file())
                    .count()
            } else {
                0
            }
        }
        _ => 0,
    };
    let diff = compute_diff(agent, cfg);
    let has_action = diff.iter().any(|d| matches!(d.kind.as_str(), "to_create" | "to_remove" | "to_replace" | "expand_parent_link"));
    let drift = has_action || parent_internal;
    let message = if !dir_ok {
        "目录不存在".into()
    } else if parent_internal {
        if parent_visible > 0 {
            format!("整目录链接模式（{} 个技能经父链接可见，待迁移）", parent_visible)
        } else {
            "整目录链接模式（待迁移）".into()
        }
    } else if let Some(pt) = &links.parent_link {
        format!("目录已重定向（skills → {}），本工具不触碰", pt.display())
    } else if drift {
        "与配置不一致，待部署".into()
    } else {
        format!("已就绪（{} 个技能链接）", linked_count)
    };
    AgentStatus {
        agent: agent.clone(),
        dir_ok,
        is_parent_link: parent_internal,
        enabled_count,
        linked_count,
        parent_visible,
        external_count,
        drift,
        message,
    }
}

/// 把 Agent 当前实际可见的技能（整目录链接内 + 独立链接，指向技能库内的）
/// 解析为技能 key 列表，用于"导入现有链接为配置"
pub fn import_existing(agent: &AgentDef, cfg: &AppConfig) -> Vec<String> {
    let skills = crate::commands::scan::scan_skills(&cfg.skills_root);
    let mut keys: Vec<String> = Vec::new();
    let links = scan_agent_links(Path::new(&agent.skills_dir));

    // 1. 整目录链接（父链接）下可见的技能子目录
    if let Some(pt) = &links.parent_link {
        if within_root(&cfg.skills_root, pt) {
            if let Ok(rd) = fs::read_dir(pt) {
                for e in rd.flatten() {
                    let p = e.path();
                    if p.join("SKILL.md").is_file() {
                        if let Some(k) = find_key_by_path(&skills, &p) {
                            keys.push(k);
                        }
                    }
                }
            }
        }
    }
    // 2. 独立链接
    for (_, kind, tgt) in &links.entries {
        if kind == "link" {
            if let Some(t) = tgt {
                if within_root(&cfg.skills_root, t) {
                    if let Some(k) = find_key_by_path(&skills, t) {
                        keys.push(k);
                    }
                }
            }
        }
    }
    keys.sort();
    keys.dedup();
    keys
}

/// 在技能库扫描结果中按路径精确匹配技能 key
fn find_key_by_path(skills: &[SkillInfo], p: &Path) -> Option<String> {
    let norm = normalize(p);
    skills
        .iter()
        .find(|s| normalize(Path::new(&s.path)) == norm)
        .map(|s| s.key.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentDef, AppConfig};
    use std::collections::HashMap;

    fn write_skill(root: &std::path::Path, cat: &str, name: &str) {
        let p = root.join(cat).join(name);
        std::fs::create_dir_all(&p).unwrap();
        std::fs::write(
            p.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: 测试\n---\n正文"),
        )
        .unwrap();
    }

    #[test]
    fn junction_create_and_remove() {
        let tmp = std::env::temp_dir().join("ash_test_junction");
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(tmp.join("target")).unwrap();
        let link = tmp.join("link");
        create_junction(&link, &tmp.join("target")).unwrap();
        assert!(is_junction(&link), "应为 junction");
        assert_eq!(junction_target(&link), Some(tmp.join("target")));
        remove_junction(&link).unwrap();
        assert!(!link.exists(), "链接应已删除");
        assert!(tmp.join("target").is_dir(), "目标目录应保留");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn diff_parent_link_expansion() {
        let tmp = std::env::temp_dir().join("ash_test_diff");
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("lib");
        std::fs::create_dir_all(&root).unwrap();
        write_skill(&root, "common", "alpha");
        write_skill(&root, "common", "beta");
        write_skill(&root, "开发", "code");

        let agent_dir = tmp.join("agent_skills");
        create_junction(&agent_dir, &root.join("common")).unwrap();

        let cfg = AppConfig {
            skills_root: root.to_string_lossy().to_string(),
            agents: Vec::new(),
            agent_configs: HashMap::from([(
                "test".into(),
                crate::models::AgentConfig {
                    enabled_skills: vec!["开发/code".into()],
                },
            )]),
            audit: Vec::new(),
            market_sources: Vec::new(),
            skill_meta: std::collections::HashMap::new(),
        };
        let agent = AgentDef {
            id: "test".into(),
            name: "Test".into(),
            skills_dir: agent_dir.to_string_lossy().to_string(),
            builtin: true,
        };

        let diff = compute_diff(&agent, &cfg);
        assert!(
            diff.iter().any(|d| d.kind == "expand_parent_link"),
            "应识别整目录链接迁移"
        );
        assert!(
            diff.iter().any(|d| d.kind == "to_create" && d.skill_key == "开发/code"),
            "应创建 开发/code 链接"
        );
        // 整目录链接模式下 _common 子技能经父链接暴露，展开父链接后随之消失，无需逐个移除

        // 执行部署并验证
        let result = deploy(&agent, &cfg, false);
        assert!(result.ok, "部署应成功: {}", result.summary);
        assert!(is_junction(&agent_dir.join("code")), "code 链接应已创建");
        assert!(!is_junction(&agent_dir), "整目录链接应已移除");
        assert!(agent_dir.is_dir(), "skills 目录应为真实目录");
        assert!(!agent_dir.join("alpha").exists(), "父链接移除后 alpha 不再可见");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rollback_undoes_creation() {
        let tmp = std::env::temp_dir().join("ash_test_rb");
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("lib");
        std::fs::create_dir_all(&root).unwrap();
        write_skill(&root, "开发", "code");

        let agent_dir = tmp.join("agent_skills");
        std::fs::create_dir_all(&agent_dir).unwrap();
        let agent = AgentDef {
            id: "test".into(),
            name: "Test".into(),
            skills_dir: agent_dir.to_string_lossy().to_string(),
            builtin: true,
        };
        let cfg = AppConfig {
            skills_root: root.to_string_lossy().to_string(),
            agents: Vec::new(),
            agent_configs: HashMap::from([(
                "test".into(),
                crate::models::AgentConfig {
                    enabled_skills: vec!["开发/code".into()],
                },
            )]),
            audit: Vec::new(),
            market_sources: Vec::new(),
            skill_meta: std::collections::HashMap::new(),
        };
        let result = deploy(&agent, &cfg, false);
        assert!(result.ok);
        assert!(is_junction(&agent_dir.join("code")));

        // 记录部署动作到审计
        let mut cfg2 = cfg.clone();
        cfg2.audit.push(crate::models::AuditEntry {
            id: "t".into(),
            ts: 1,
            action: "deploy".into(),
            agent_id: "test".into(),
            detail: "test".into(),
            actions: result.actions.clone(),
        });

        // 回滚应撤销创建 → 移除链接
        let rb = rollback_last(&cfg2, "test");
        assert!(rb.ok, "回滚应成功: {}", rb.summary);
        assert!(
            !agent_dir.join("code").exists(),
            "回滚后已创建的链接应被移除"
        );
        assert!(root.join("开发").join("code").is_dir(), "源文件不受影响");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn rollback_restores_removed() {
        let tmp = std::env::temp_dir().join("ash_test_rb2");
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("lib");
        std::fs::create_dir_all(&root).unwrap();
        write_skill(&root, "开发", "code");
        let agent_dir = tmp.join("agent_skills");
        std::fs::create_dir_all(&agent_dir).unwrap();

        // 构造一条包含 remove_link 的审计（目标仍存在）
        let cfg = AppConfig {
            skills_root: root.to_string_lossy().to_string(),
            agents: Vec::new(),
            agent_configs: HashMap::new(),
            market_sources: Vec::new(),
            skill_meta: std::collections::HashMap::new(),
            audit: vec![crate::models::AuditEntry {
                id: "t".into(),
                ts: 1,
                action: "deploy".into(),
                agent_id: "test".into(),
                detail: "test".into(),
                actions: vec![DeployAction {
                    kind: "remove_link".into(),
                    skill_key: "开发/code".into(),
                    link_path: agent_dir.join("code").to_string_lossy().to_string(),
                    target: root.join("开发").join("code").to_string_lossy().to_string(),
                    ok: true,
                    message: "test".into(),
                }],
            }],
        };
        let rb = rollback_last(&cfg, "test");
        assert!(rb.ok, "回滚应成功: {}", rb.summary);
        assert!(is_junction(&agent_dir.join("code")), "回滚应重建被移除的链接");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn import_existing_from_parent_link() {
        let tmp = std::env::temp_dir().join("ash_test_import");
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("lib");
        std::fs::create_dir_all(&root).unwrap();
        write_skill(&root, "common", "alpha");
        write_skill(&root, "common", "beta");
        write_skill(&root, "开发", "code");

        let agent_dir = tmp.join("agent_skills");
        create_junction(&agent_dir, &root.join("common")).unwrap();

        let cfg = AppConfig {
            skills_root: root.to_string_lossy().to_string(),
            agents: Vec::new(),
            agent_configs: HashMap::new(),
            audit: Vec::new(),
            market_sources: Vec::new(),
            skill_meta: std::collections::HashMap::new(),
        };
        let agent = AgentDef {
            id: "test".into(),
            name: "Test".into(),
            skills_dir: agent_dir.to_string_lossy().to_string(),
            builtin: true,
        };
        let keys = import_existing(&agent, &cfg);
        assert!(keys.contains(&"common/alpha".to_string()), "应导入 common/alpha: {keys:?}");
        assert!(keys.contains(&"common/beta".to_string()), "应导入 common/beta: {keys:?}");
        assert!(!keys.contains(&"开发/code".to_string()), "独立链接未部署时不应导入 开发/code: {keys:?}");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn import_existing_from_individual_links() {
        let tmp = std::env::temp_dir().join("ash_test_import2");
        let _ = std::fs::remove_dir_all(&tmp);
        let root = tmp.join("lib");
        std::fs::create_dir_all(&root).unwrap();
        write_skill(&root, "开发", "code");

        let agent_dir = tmp.join("agent_skills");
        std::fs::create_dir_all(&agent_dir).unwrap();
        create_junction(&agent_dir.join("code"), &root.join("开发").join("code")).unwrap();

        let cfg = AppConfig {
            skills_root: root.to_string_lossy().to_string(),
            agents: Vec::new(),
            agent_configs: HashMap::new(),
            audit: Vec::new(),
            market_sources: Vec::new(),
            skill_meta: std::collections::HashMap::new(),
        };
        let agent = AgentDef {
            id: "test".into(),
            name: "Test".into(),
            skills_dir: agent_dir.to_string_lossy().to_string(),
            builtin: true,
        };
        let keys = import_existing(&agent, &cfg);
        assert_eq!(keys, vec!["开发/code".to_string()], "应导入独立链接对应的技能 key");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
