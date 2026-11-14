//! 从技能库删除技能（移入 Windows 回收站）
//!
//! 删除一个技能是「破坏性」操作，这里的原则是：
//! 1. 只允许删除技能库根目录内、且含 SKILL.md 的技能目录；越界一律拒绝
//! 2. 先清理各 Agent 里指向它的 junction 链接，再从 Agent 配置里摘除，最后才动技能库
//! 3. 只移入系统回收站（可还原），绝不硬删除；回收站失败则原样保留并报错

use crate::commands::deploy::{is_junction, junction_target, normalize, remove_junction, within_root};
use crate::commands::scan::resolve_skill_target;
use crate::commands::hidden_command;
use crate::models::*;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

/// 回收站脚本：纯 ASCII，待删路径通过环境变量 ASH_DELETE_PATH 传入，
/// 避免把中文 / 特殊字符路径拼进命令行引发编码与转义问题。
/// 策略一：.NET 官方 API；策略二（兜底）：资源管理器 Shell COM 的静默 MoveHere。
const RECYCLE_PS: &str = "$ErrorActionPreference='Stop'; $p=$env:ASH_DELETE_PATH; $done=$false; try { Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory($p,[Microsoft.VisualBasic.FileIO.UIOption]::OnlyErrorDialogs,[Microsoft.VisualBasic.FileIO.RecycleOption]::SendToRecycleBin); $done = -not (Test-Path -LiteralPath $p) } catch { $done=$false }; if (-not $done) { try { $sh=New-Object -ComObject Shell.Application; $sh.NameSpace(10).MoveHere($p,(4 -bor 16 -bor 1024)); $done = -not (Test-Path -LiteralPath $p) } catch { $done=$false } }; if ($done) { Write-Output 'OK' } else { Write-Output 'FAIL' }";

/// 技能库路径比较：Windows 大小写不敏感 + 统一分隔符
fn same_path(a: &Path, b: &Path) -> bool {
    normalize(a).to_string_lossy().to_lowercase() == normalize(b).to_string_lossy().to_lowercase()
}

/// 带超时地等待子进程结束（防止系统弹窗把应用卡死）
fn run_with_timeout(cmd: &mut std::process::Command, secs: u64) -> Result<std::process::Output, String> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 powershell 失败: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if start.elapsed() > Duration::from_secs(secs) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("移入回收站超时（可能有系统窗口在等待确认），已取消，未做任何改动".into());
                }
                std::thread::sleep(Duration::from_millis(120));
            }
            Err(e) => return Err(format!("等待 powershell 结束失败: {e}")),
        }
    }
    child
        .wait_with_output()
        .map_err(|e| format!("读取 powershell 输出失败: {e}"))
}

/// 把目录移入 Windows 回收站；失败则保持原样返回 Err（绝不硬删）
fn move_to_recycle_bin(path: &Path) -> Result<(), String> {
    let mut cmd = hidden_command("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", RECYCLE_PS])
        .env("ASH_DELETE_PATH", path.to_string_lossy().to_string());
    let out = run_with_timeout(&mut cmd, 25)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    if stdout.contains("OK") && !path.exists() {
        return Ok(());
    }
    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if err.is_empty() {
        Err("系统回收站拒绝了该操作（目录可能被其他程序占用）".into())
    } else {
        Err(format!(
            "系统回收站操作失败: {}",
            err.chars().take(300).collect::<String>()
        ))
    }
}

/// 应用内回收站目录：<skills_root>/_archive/trash
fn trash_dir(root: &str) -> std::path::PathBuf {
    Path::new(root).join("_archive").join("trash")
}

/// 回收站支持旧的平铺名称与多层分类路径，拒绝越界和链接目录。
fn trash_item_path(root: &str, name: &str) -> Result<std::path::PathBuf, String> {
    let parts: Vec<_> = name.split('/').collect();
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()
        || crate::commands::scan::validate_category(part).is_err()) {
        return Err(format!("非法回收站条目: {name}"));
    }
    let mut path = Path::new(root).join("_archive");
    if is_junction(&path) { return Err("归档目录是链接，拒绝操作".into()); }
    path.push("trash");
    if is_junction(&path) { return Err("回收站目录是链接，拒绝操作".into()); }
    let count = parts.len();
    for (index, part) in parts.into_iter().enumerate() {
        path.push(part);
        if is_junction(&path) { return Err("回收站条目或分类是链接，拒绝操作".into()); }
        if index + 1 < count && path.join("SKILL.md").is_file() {
            return Err("回收站分类位置已被旧技能条目占用，未改动".into());
        }
    }
    Ok(path)
}

fn trash_skill_key(name: &str) -> String {
    // 兼容旧的 分类__技能-时间戳；新条目直接保留相对分类路径。
    let mut key = if name.contains('/') { name.to_string() } else { name.replace("__", "/") };
    if let Some(idx) = key.rfind('-') {
        let suffix = &key[idx + 1..];
        if suffix.len() >= 10 && suffix.chars().all(|c| c.is_ascii_digit()) { key.truncate(idx); }
    }
    key
}

/// 把技能目录移入应用内回收站（同盘 rename，瞬时完成）。
/// 重名时追加时间戳后缀。返回回收站里的条目名。
pub fn move_to_trash(root: &str, target: &Path, key: &str) -> Result<String, String> {
    let dest = trash_item_path(root, key)?;
    fs::create_dir_all(dest.parent().ok_or("回收站目录无效")?).map_err(|e| format!("创建回收站目录失败: {e}"))?;
    let mut name = key.to_string();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut collision = 0u32;
    while fs::symlink_metadata(trash_dir(root).join(&name)).is_ok() {
        name = if collision == 0 { format!("{key}-{ts}") } else { format!("{key}-{ts}{collision:03}") };
        collision += 1;
    }
    let dest = trash_item_path(root, &name)?;
    fs::rename(target, &dest).map_err(|e| {
        format!(
            "移入回收站失败（目录可能被占用）: {e}。原目录未改动：{}",
            target.display()
        )
    })?;
    Ok(name)
}

/// 列出应用内回收站里的技能
pub fn list_trash(root: &str) -> Vec<crate::models::TrashItem> {
    use crate::models::TrashItem;
    let mut out = Vec::new();
    let tdir = trash_dir(root);
    let mut entries = Vec::new();
    crate::commands::scan::visit_skills(&tdir.to_string_lossy(), |path, _, _, name| {
        if trash_item_path(root, &name).is_ok() { entries.push((name, path)); }
    });
    for (name, p) in entries {
        let key = trash_skill_key(&name);
        let deleted_ts = p
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        out.push(TrashItem {
            name,
            key,
            size_bytes: crate::commands::scan::dir_size_pub(&p),
            deleted_ts,
            path: p.to_string_lossy().to_string(),
        });
    }
    out.sort_by_key(|item| std::cmp::Reverse(item.deleted_ts));
    out
}

/// 从应用内回收站还原技能到原位置
pub fn restore_from_trash(root: &str, trash_name: &str) -> Result<String, String> {
    let src = trash_item_path(root, trash_name)?;
    if !src.join("SKILL.md").is_file() {
        return Err(format!("回收站中不存在: {trash_name}"));
    }
    let key = trash_skill_key(trash_name);
    // 校验解码后的 key；旧编码也不能绕过路径和保留名称检查。
    trash_item_path(root, &key)?;
    let dest = Path::new(root).join(&key);
    if fs::symlink_metadata(&dest).is_ok() {
        return Err(format!(
            "原位置已被同名技能占用：{}。请先处理冲突再还原",
            dest.display()
        ));
    }
    if let Some(parent) = dest.parent() {
        let mut wrapper = Path::new(root).to_path_buf();
        for part in parent.strip_prefix(root).map_err(|_| "还原路径不在技能库内")?.components() {
            wrapper.push(part);
            if is_junction(&wrapper) || wrapper.join("SKILL.md").is_file() {
                return Err("还原分类目录是链接或技能目录，未覆盖".into());
            }
        }
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {e}"))?;
    }
    fs::rename(&src, &dest).map_err(|e| format!("还原失败: {e}"))?;
    Ok(key)
}

/// 彻底删除回收站里的技能（移入 Windows 回收站，仍可从系统还原）
pub fn purge_trash(root: &str, trash_name: &str) -> Result<(), String> {
    let p = trash_item_path(root, trash_name)?;
    if !p.join("SKILL.md").is_file() {
        return Err(format!("回收站中不存在: {trash_name}"));
    }
    move_to_recycle_bin(&p)
}

/// 删除技能库中的一个技能（含链接与配置清理），成功后移入回收站
#[cfg(test)]
pub fn delete_skill(cfg: &mut AppConfig, key: &str) -> Result<DeleteResult, String> {
    let allowed=cfg.agents.iter().filter(|a| super::agent_policy::enabled(cfg,&a.id)).map(|a| a.id.clone()).collect();
    delete_skill_with_access(cfg,key,&allowed)
}

pub fn delete_skill_with_access(cfg: &mut AppConfig, key: &str, allowed: &std::collections::HashSet<String>) -> Result<DeleteResult, String> {
    let root = cfg.skills_root.clone();
    if root.trim().is_empty() {
        return Err("技能库路径未设置".into());
    }
    // 非法标识（路径穿越 / 盘符 / 绝对路径）
    if key.contains("..") || key.contains(':') || key.starts_with('\\') || key.starts_with('/') {
        return Err(format!("拒绝删除：非法的技能标识 {key}"));
    }
    let target = resolve_skill_target(&root, key)
        .ok_or_else(|| format!("技能不存在或已被删除：{key}"))?;

    // 安全检查 1：必须落在技能库根目录内
    if !within_root(&root, &target) {
        return Err(format!("拒绝删除：路径不在技能库内 {}", target.display()));
    }
    // 安全检查 2：不能是技能库根目录本身
    if same_path(&target, Path::new(&root)) {
        return Err("拒绝删除：这是技能库根目录".into());
    }
    // 安全检查 3：必须是技能目录（含 SKILL.md），避免误删分类目录
    if !target.join("SKILL.md").is_file() {
        return Err(format!(
            "拒绝删除：{} 不是技能目录（缺少 SKILL.md）",
            target.display()
        ));
    }

    // 关闭管理后不再触碰其链接和分配。删除共享源也会影响这些工具，因此在任何写入之前拒绝。
    for a in cfg.agents.iter().filter(|a| !allowed.contains(&a.id)) {
        let assigned=cfg.agent_configs.get(&a.id).is_some_and(|ac| ac.enabled_skills.iter().any(|k|
            k==key || resolve_skill_target(&root,k).is_some_and(|p| same_path(&p,&target))));
        let dir=Path::new(&a.skills_dir);
        let parent_uses_root=is_junction(dir) && junction_target(dir).is_some_and(|p| same_path(&p,Path::new(&root)) || within_root(&p.to_string_lossy(),&target));
        let linked=fs::read_dir(dir).into_iter().flatten().flatten().any(|e| is_junction(&e.path()) && junction_target(&e.path()).is_some_and(|p| same_path(&p,&target)));
        if assigned || parent_uses_root || linked { return Err(format!("{} 尚未允许配置且仍使用此技能，请先在设置中开启后处理其分配和链接",a.name)); }
    }
    let agents:Vec<_> = cfg.agents.iter().filter(|a| allowed.contains(&a.id)).cloned().collect();
    let mut removed_links: Vec<DeployAction> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // 1) 清理各 Agent skills 目录里指向该技能的 junction
    for a in &agents {
        let dir = Path::new(&a.skills_dir);
        if !dir.is_dir() {
            continue;
        }
        // 整目录链接模式：skills 本身是链接，技能随目标目录消失，无需逐条清理
        if is_junction(dir) {
            continue;
        }
        let Ok(rd) = fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !is_junction(&p) {
                continue;
            }
            let Some(t) = junction_target(&p) else { continue };
            if same_path(&t, &target) {
                match remove_junction(&p) {
                    Ok(()) => removed_links.push(DeployAction {
                        kind: "remove_link".into(),
                        skill_key: key.to_string(),
                        link_path: p.to_string_lossy().to_string(),
                        target: target.to_string_lossy().to_string(),
                        ok: true,
                        message: format!("{} 的链接已移除", a.name),
                    }),
                    Err(err) => warnings.push(format!("{} 的链接清理失败：{err}", a.name)),
                }
            }
        }
    }

    // 2) 从各 Agent 配置里摘除该技能（按解析后的真实路径精确匹配，避免同名误删）
    let mut unassigned_agents: Vec<String> = Vec::new();
    for (agent_id, ac) in cfg.agent_configs.iter_mut() {
        if !allowed.contains(agent_id) { continue; }
        let before = ac.enabled_skills.len();
        ac.enabled_skills.retain(|k| match resolve_skill_target(&root, k) {
            Some(p) => !same_path(&p, &target),
            // 解析不到路径的技能（目录已缺失）退回 key 比较
            None => k != key,
        });
        if ac.enabled_skills.len() != before {
            unassigned_agents.push(
                agents
                    .iter()
                    .find(|a| &a.id == agent_id)
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| agent_id.clone()),
            );
        }
    }

    // 3) 移入应用内回收站（技能库里的实体目录；若该条目本身是链接，则只删链接）
    // 应用内回收站支持一键还原；「彻底删除」时才进 Windows 回收站
    let recycled = if is_junction(&target) {
        remove_junction(&target)?;
        false
    } else {
        move_to_trash(&root, &target, key)?;
        true
    };

    if target.exists() {
        return Err(format!(
            "删除后目录仍然存在，请手动检查：{}",
            target.display()
        ));
    }

    let mut message = if recycled {
        format!("已将 {key} 移入应用内回收站，可随时还原")
    } else {
        format!("已移除链接 {key}（原目标未被触碰）")
    };
    if !removed_links.is_empty() {
        message = format!("{message}，清理 {} 个 Agent 链接", removed_links.len());
    }
    if !unassigned_agents.is_empty() {
        message = format!("{message}，并从 {} 个 Agent 配置中摘除", unassigned_agents.len());
    }
    if !warnings.is_empty() {
        message = format!("{message}（{} 项告警）", warnings.len());
    }

    Ok(DeleteResult {
        skill_key: key.to_string(),
        path: target.to_string_lossy().to_string(),
        recycled,
        removed_links,
        unassigned_agents,
        warnings,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AgentConfig, AgentDef, AppConfig};
    use std::collections::HashMap;

    fn write_skill(root: &Path, name: &str) -> std::path::PathBuf {
        let p = root.join(name);
        fs::create_dir_all(&p).unwrap();
        fs::write(
            p.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: 测试\n---\n正文"),
        )
        .unwrap();
        p
    }

    fn base_cfg(root: &Path, agent_dir: &Path) -> AppConfig {
        AppConfig {
            skill_link_backups: Vec::new(),
            skills_root: root.to_string_lossy().to_string(),
            agents: vec![AgentDef {
                id: "test".into(),
                name: "Test".into(),
                skills_dir: agent_dir.to_string_lossy().to_string(),
                builtin: true,
            }],
            agent_configs: HashMap::from([(
                "test".into(),
                AgentConfig {
                    enabled_skills: vec!["alpha".into(), "beta".into()],
                },
            )]),
            audit: Vec::new(),
            market_sources: Vec::new(),
            skill_meta: std::collections::HashMap::new(),
            token_budget: 8000,
            agent_preferences: HashMap::new(),
            builtin_sources: Vec::new(),
        }
    }

    #[test]
    fn categorized_trash_lists_legacy_entries_and_restores_to_original_category() {
        let root = crate::commands::test_scratch("categorized_trash");
        let skill = write_skill(&root, "05-文档知识库/doc");
        let root_str = root.to_string_lossy();
        let name = move_to_trash(&root_str, &skill, "05-文档知识库/doc").unwrap();
        assert_eq!(name, "05-文档知识库/doc");
        assert!(root.join("_archive/trash/05-文档知识库/doc/SKILL.md").is_file());
        write_skill(&root, "_archive/trash/开发__legacy-1790000000");
        let items = list_trash(&root_str);
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|item| item.key == "05-文档知识库/doc"));
        assert!(items.iter().any(|item| item.key == "开发/legacy"));
        assert_eq!(restore_from_trash(&root_str, &name).unwrap(), "05-文档知识库/doc");
        assert!(skill.join("SKILL.md").is_file());
        assert_eq!(restore_from_trash(&root_str, "开发__legacy-1790000000").unwrap(), "开发/legacy");
        assert!(list_trash(&root_str).is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn categorized_trash_blocks_traversal_conflicts_and_category_operations() {
        let root = crate::commands::test_scratch("trash_boundaries");
        let source = write_skill(&root, "分类/doc");
        let root_str = root.to_string_lossy();
        let name = move_to_trash(&root_str, &source, "分类/doc").unwrap();
        write_skill(&root, "分类/doc");
        assert!(restore_from_trash(&root_str, &name).unwrap_err().contains("占用"));
        assert!(restore_from_trash(&root_str, "分类").is_err());
        assert!(purge_trash(&root_str, "分类").is_err());
        for invalid in ["../doc", "分类/../../doc", "分类\\doc", "C:/doc", "/doc", "a/b/c", "开发__..__doc"] {
            assert!(restore_from_trash(&root_str, invalid).is_err(), "{invalid}");
        }
        // 同一技能多次进入回收站时，每个条目均还原成相同的原 key。
        for _ in 0..3 {
            let entry = move_to_trash(&root_str, &source, "分类/doc").unwrap();
            assert_eq!(trash_skill_key(&entry), "分类/doc");
            write_skill(&root, "分类/doc");
        }
        assert_eq!(list_trash(&root_str).len(), 4);
        let source = write_skill(&root, "occupied/extra");
        write_skill(&root, "_archive/trash/occupied");
        assert!(move_to_trash(&root_str, &source, "occupied/extra").is_err());
        assert!(source.join("SKILL.md").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn categorized_trash_refuses_linked_categories_and_restore_targets() {
        let tmp = crate::commands::test_scratch("trash_links");
        let root = tmp.join("library");
        let outside = tmp.join("outside");
        write_skill(&root, "_archive/trash/分类/doc");
        fs::create_dir_all(&outside).unwrap();
        crate::commands::deploy::create_junction(&root.join("分类"), &outside).unwrap();
        let text = root.to_string_lossy();
        assert!(restore_from_trash(&text, "分类/doc").is_err());
        assert!(!outside.join("doc").exists());
        remove_junction(&root.join("分类")).unwrap();
        crate::commands::deploy::create_junction(&root.join("_archive/trash/外部"), &outside).unwrap();
        assert!(purge_trash(&text, "外部/doc").is_err());
        assert_eq!(list_trash(&text).len(), 1);
        remove_junction(&root.join("_archive/trash/外部")).unwrap();
        fs::remove_dir_all(tmp).unwrap();
    }

    /// 回收站通道本身可用（端到端真实调用一次系统回收站）
    #[test]
    fn recycle_bin_roundtrip() {
        let tmp = crate::commands::test_scratch("del_recycle");
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("SKILL.md"), "---\nname: probe\n---\n").unwrap();
        move_to_recycle_bin(&tmp).expect("应能移入回收站");
        assert!(!tmp.exists(), "目录应已进入回收站");
    }

    /// 拒绝删除技能库根目录
    #[test]
    fn refuse_root_dir() {
        let tmp = crate::commands::test_scratch("del_root");
        fs::create_dir_all(&tmp).unwrap();
        let mut cfg = base_cfg(&tmp, &tmp.join("agent"));
        // 用 ".." 之类穿越标识必须被拒绝
        assert!(delete_skill(&mut cfg, "../evil").is_err(), "路径穿越应被拒绝");
        assert!(delete_skill(&mut cfg, "E:/x").is_err(), "带盘符标识应被拒绝");
        let _ = fs::remove_dir_all(&tmp);
    }

    /// 缺少 SKILL.md 的目录拒绝删除
    #[test]
    fn refuse_non_skill_dir() {
        let tmp = crate::commands::test_scratch("del_nonskill");
        fs::create_dir_all(tmp.join("notaskill")).unwrap();
        let mut cfg = base_cfg(&tmp, &tmp.join("agent"));
        let err = delete_skill(&mut cfg, "notaskill").unwrap_err();
        assert!(err.contains("不是技能目录"), "应提示不是技能目录: {err}");
        assert!(tmp.join("notaskill").is_dir(), "目录应原样保留");
        let _ = fs::remove_dir_all(&tmp);
    }

    /// 缺失技能应报错
    #[test]
    fn missing_skill_errors() {
        let tmp = crate::commands::test_scratch("del_missing");
        fs::create_dir_all(&tmp).unwrap();
        let mut cfg = base_cfg(&tmp, &tmp.join("agent"));
        assert!(delete_skill(&mut cfg, "nope").is_err());
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn closed_agent_keeps_link_assignment_and_shared_source() {
        let tmp=crate::commands::test_scratch("delete_closed");
        let root=tmp.join("lib"); let agent=tmp.join("agent");
        fs::create_dir_all(&root).unwrap(); fs::create_dir_all(&agent).unwrap();
        write_skill(&root,"alpha"); write_skill(&root,"beta");
        crate::commands::deploy::create_junction(&agent.join("alpha"),&root.join("alpha")).unwrap();
        let mut cfg=base_cfg(&root,&agent);
        cfg.agent_preferences.entry("test".into()).or_default();
        let error=delete_skill(&mut cfg,"alpha").unwrap_err();
        assert!(error.contains("尚未允许配置"));
        assert!(root.join("alpha/SKILL.md").is_file());
        assert!(is_junction(&agent.join("alpha")));
        assert_eq!(cfg.agent_configs["test"].enabled_skills,["alpha","beta"]);
        let _=crate::commands::deploy::remove_junction(&agent.join("alpha"));
        let _=fs::remove_dir_all(tmp);
    }

    /// 校验前置逻辑：链接清理 + 配置摘除（不触发回收站，改用 junction 分支）
    #[test]
    fn cleans_links_and_config() {
        let tmp = crate::commands::test_scratch("del_clean");
        let root = tmp.join("lib");
        let agent_dir = tmp.join("agent");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();
        write_skill(&root, "alpha");
        write_skill(&root, "beta");
        // alpha 已部署到 Agent
        crate::commands::deploy::create_junction(&agent_dir.join("alpha"), &root.join("alpha")).unwrap();

        let mut cfg = base_cfg(&root, &agent_dir);
        let res = delete_skill(&mut cfg, "alpha").unwrap();
        assert_eq!(res.path.to_lowercase(), root.join("alpha").to_string_lossy().to_lowercase());
        assert_eq!(res.removed_links.len(), 1, "应清理 1 个链接: {:?}", res.removed_links);
        assert!(!agent_dir.join("alpha").exists(), "链接应已移除");
        assert_eq!(res.unassigned_agents, vec!["Test".to_string()], "应从 Test 配置摘除");
        assert_eq!(cfg.agent_configs["test"].enabled_skills, vec!["beta".to_string()], "beta 应保留");
        assert!(root.join("beta").is_dir(), "其他技能不受影响");
        assert!(!root.join("alpha").exists(), "技能目录应已移走");
        let _ = fs::remove_dir_all(&tmp);
    }
}
