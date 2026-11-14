//! 用户显式预览后，将实体技能保留为备份并替换为技能库链接。
use super::{assignments, deploy};
use crate::models::{AppConfig, SkillLinkBackup};
use serde::Serialize;
use std::{
    collections::hash_map::DefaultHasher,
    fs,
    hash::{Hash, Hasher},
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
pub struct Preview {
    pub agent_id: String,
    pub skill_key: String,
    pub source_key: String,
    pub original_path: String,
    pub link_path: String,
    pub target: String,
    pub fingerprint: String,
}

// 包含所有文件内容；不跟随目录内链接，避免把库外来源或循环混入备份。
pub(super) fn fingerprint(path: &Path) -> Result<String, String> {
    fn visit(
        root: &Path,
        path: &Path,
        hash: &mut DefaultHasher,
        count: &mut u64,
        bytes: &mut u64,
    ) -> Result<(), String> {
        let meta =
            fs::symlink_metadata(path).map_err(|e| format!("无法读取 {}：{e}", path.display()))?;
        if meta.file_type().is_symlink() || deploy::is_junction(path) {
            return Err("技能包含链接，请先在原工具中处理后再转换".into());
        }
        *count += 1;
        if *count > 20000 {
            return Err("技能文件过多，暂不支持自动转换".into());
        }
        path.strip_prefix(root)
            .map_err(|e| e.to_string())?
            .hash(hash);
        meta.is_dir().hash(hash);
        if meta.is_dir() {
            let mut entries = fs::read_dir(path)
                .map_err(|e| e.to_string())?
                .map(|e| e.map(|e| e.path()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            entries.sort();
            for entry in entries {
                visit(root, &entry, hash, count, bytes)?;
            }
        } else if meta.is_file() {
            *bytes += meta.len();
            if *bytes > 256 * 1024 * 1024 {
                return Err("技能大于 256 MB，请手动迁移".into());
            }
            meta.len().hash(hash);
            let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
            let mut buffer = [0; 65536];
            loop {
                let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                hash.write(&buffer[..n]);
            }
        } else {
            return Err("技能包含不支持的特殊文件".into());
        }
        Ok(())
    }
    let mut hash = DefaultHasher::new();
    visit(path, path, &mut hash, &mut 0, &mut 0)?;
    Ok(format!("{:016x}", hash.finish()))
}

fn user_dir(cfg: &AppConfig, id: &str) -> Result<PathBuf, String> {
    let agent = cfg
        .agents
        .iter()
        .find(|a| a.id == id)
        .ok_or("Agent 不存在")?;
    let path = Path::new(&agent.skills_dir);
    if !path.is_absolute() || !path.is_dir() || deploy::is_junction(path) {
        return Err("仅支持独立的 Agent 用户技能目录".into());
    }
    let dir = fs::canonicalize(path).map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn preview_source(
    cfg: &AppConfig,
    id: &str,
    key: &str,
    source_key: Option<&str>,
) -> Result<Preview, String> {
    let target = assignments::skill_target(&cfg.skills_root, key)?;
    let dir = user_dir(cfg, id)?;
    let mut candidates = Vec::new();
    super::scan::visit_skills(&dir.to_string_lossy(), |path, _, _, candidate_key| {
        if path.file_name() == target.file_name()
            && source_key.is_none_or(|key| key == candidate_key)
        {
            candidates.push((path, candidate_key));
        }
    });
    if candidates.len() != 1 {
        return Err("没有唯一的同名实体技能，请在 Agent 自有技能列表中选择具体来源后重试".into());
    }
    let (source, source_key) = candidates.remove(0);
    check_parent(&dir, &source)?;
    let link = dir.join(target.file_name().ok_or("技能名为空")?);
    if link != source && fs::symlink_metadata(&link).is_ok() {
        return Err("Agent 根目录已有同名内容，请先检查；不会覆盖".into());
    }
    let library = fs::canonicalize(&cfg.skills_root).map_err(|e| e.to_string())?;
    let actual_source = fs::canonicalize(&source).map_err(|e| e.to_string())?;
    if deploy::normalize(&actual_source).starts_with(deploy::normalize(&library))
        || deploy::normalize(&library).starts_with(deploy::normalize(&actual_source))
    {
        return Err("来源与技能库有包含关系，不能转换".into());
    }
    if !source.join("SKILL.md").is_file() {
        return Err("没有同名实体技能，可能已被外部修改，请刷新".into());
    }
    let source_hash = fingerprint(&source)?;
    let target_hash = fingerprint(&target)?;
    Ok(Preview {
        agent_id: id.into(),
        skill_key: key.into(),
        source_key,
        original_path: source.to_string_lossy().into_owned(),
        link_path: link.to_string_lossy().into_owned(),
        target: target.to_string_lossy().into_owned(),
        fingerprint: format!("{source_hash}:{target_hash}"),
    })
}

#[cfg(test)]
fn preview(cfg: &AppConfig, id: &str, key: &str) -> Result<Preview, String> {
    preview_source(cfg, id, key, None)
}

fn check_parent(dir: &Path, source: &Path) -> Result<PathBuf, String> {
    let relative = source
        .strip_prefix(dir)
        .map_err(|_| "原目录不在当前 Agent 用户目录内")?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("原目录位置无效".into());
    }
    let parent = source.parent().ok_or("技能无父目录")?;
    for ancestor in parent.ancestors().take_while(|p| *p != dir) {
        if deploy::is_junction(ancestor)
            || fs::symlink_metadata(ancestor).is_ok_and(|m| m.file_type().is_symlink())
        {
            return Err("技能的父目录包含外部链接，不能自动转换或恢复".into());
        }
    }
    Ok(parent.to_path_buf())
}

fn backup_dir(dir: &Path) -> Result<PathBuf, String> {
    let path = dir.join(".agentskillhub-backups");
    if deploy::is_junction(&path)
        || fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink())
    {
        return Err("备份目录被外部链接替换，请先检查".into());
    }
    Ok(path)
}

pub fn apply_source(
    cfg: &AppConfig,
    id: &str,
    key: &str,
    expected: &str,
    source_key: Option<&str>,
) -> Result<SkillLinkBackup, String> {
    let plan = preview_source(cfg, id, key, source_key)?;
    if plan.fingerprint != expected {
        return Err("预览后技能内容发生变化，请重新预览；未改动原目录".into());
    }
    let source = Path::new(&plan.original_path);
    let parent = backup_dir(source.parent().ok_or("技能无父目录")?)?;
    fs::create_dir_all(&parent).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let receipt_id = format!("{stamp}-{}", std::process::id());
    let backup = parent.join(&receipt_id);
    // rename 不覆盖任何已有备份；正常目录仍完整保留在同一卷。
    if backup.exists() {
        return Err("备份标识冲突，请重试".into());
    }
    fs::rename(source, &backup).map_err(|e| format!("保留原目录失败：{e}"))?;
    if let Err(error) = deploy::create_junction(Path::new(&plan.link_path), Path::new(&plan.target))
    {
        let restore = fs::rename(&backup, source);
        return Err(format!(
            "创建链接失败：{error}；{}",
            restore
                .err()
                .map(|e| format!("原目录仍保留在 {}：{e}", backup.display()))
                .unwrap_or_else(|| "已恢复原目录".into())
        ));
    }
    Ok(SkillLinkBackup {
        id: receipt_id,
        agent_id: id.into(),
        skill_key: key.into(),
        original_path: plan.original_path,
        link_path: plan.link_path,
        backup_path: backup.to_string_lossy().into_owned(),
        target: plan.target,
        fingerprint: expected.split(':').next().unwrap_or_default().into(),
    })
}

#[cfg(test)]
fn apply(cfg: &AppConfig, id: &str, key: &str, expected: &str) -> Result<SkillLinkBackup, String> {
    apply_source(cfg, id, key, expected, None)
}

pub fn restore(cfg: &AppConfig, receipt: &SkillLinkBackup) -> Result<(), String> {
    let dir = user_dir(cfg, &receipt.agent_id)?;
    let original = Path::new(&receipt.original_path);
    let backup = Path::new(&receipt.backup_path);
    let link = Path::new(if receipt.link_path.is_empty() {
        &receipt.original_path
    } else {
        &receipt.link_path
    });
    let parent = check_parent(&dir, original)?;
    if backup.parent() != Some(backup_dir(&parent)?.as_path())
        || backup.file_name().and_then(|s| s.to_str()) != Some(&receipt.id)
        || link.parent() != Some(dir.as_path())
        || link.file_name() != original.file_name()
    {
        return Err("备份不属于当前用户技能目录，请在原位置手动恢复".into());
    }
    if fingerprint(backup)? != receipt.fingerprint {
        return Err("备份已被外部修改，请检查后手动恢复；不会覆盖".into());
    }
    if original != link && fs::symlink_metadata(original).is_ok() {
        return Err("原位置已被其他内容占用，未覆盖；请先检查".into());
    }
    let had_link = fs::symlink_metadata(link).is_ok();
    if had_link {
        if !deploy::is_junction(link)
            || !deploy::junction_target(link)
                .is_some_and(|p| deploy::same_target(&p, Path::new(&receipt.target)))
        {
            return Err("原位置已被其他内容占用，未覆盖；请先检查".into());
        }
        deploy::remove_junction(link)?;
    }
    if let Err(error) = fs::rename(backup, original) {
        let recovery = if had_link {
            deploy::create_junction(link, Path::new(&receipt.target))
        } else {
            Ok(())
        };
        return Err(format!(
            "恢复原目录失败：{error}；{}",
            recovery.err().unwrap_or_else(|| "备份保留".into())
        ));
    }
    Ok(())
}

/// 仅在恢复后保存配置失败时撤销恢复，保持磁盘和持久化记录一致。
pub fn undo_restore(receipt: &SkillLinkBackup, had_link: bool) -> Result<(), String> {
    fs::rename(&receipt.original_path, &receipt.backup_path).map_err(|e| e.to_string())?;
    if had_link {
        deploy::create_junction(
            Path::new(if receipt.link_path.is_empty() {
                &receipt.original_path
            } else {
                &receipt.link_path
            }),
            Path::new(&receipt.target),
        )
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AgentDef;
    fn fixture(tag: &str) -> (PathBuf, AppConfig) {
        let root = super::super::test_scratch(tag);
        fs::create_dir_all(root.join("library/demo")).unwrap();
        fs::create_dir_all(root.join("agent/demo/nested")).unwrap();
        fs::write(root.join("library/demo/SKILL.md"), "library version").unwrap();
        fs::write(root.join("agent/demo/SKILL.md"), "original version").unwrap();
        fs::write(root.join("agent/demo/nested/data.bin"), [0, 1, 2, 255]).unwrap();
        let mut cfg = AppConfig::default();
        cfg.skills_root = root.join("library").to_string_lossy().into();
        cfg.agents.push(AgentDef {
            id: "fixture".into(),
            name: "Fixture".into(),
            skills_dir: root.join("agent").to_string_lossy().into(),
            builtin: false,
        });
        (root, cfg)
    }
    #[test]
    fn nested_external_skill_is_backed_up_and_linked_at_agent_root() {
        let (root, cfg) = fixture("link-category");
        fs::create_dir_all(root.join("agent/market")).unwrap();
        fs::rename(root.join("agent/demo"), root.join("agent/market/demo")).unwrap();
        let plan = preview_source(&cfg, "fixture", "demo", Some("market/demo")).unwrap();
        let record = apply_source(
            &cfg,
            "fixture",
            "demo",
            &plan.fingerprint,
            Some("market/demo"),
        )
        .unwrap();
        assert!(deploy::is_junction(&root.join("agent/demo")));
        assert!(!root.join("agent/market/demo").exists());
        assert!(
            deploy::assigned_skills(&cfg.agents[0], &cfg.skills_root).contains(&"demo".to_string())
        );
        assert!(assignments::owned_skills(
            &cfg,
            &super::super::scan::scan_skills(&cfg.skills_root)
        )
        .is_empty());
        restore(&cfg, &record).unwrap();
        assert!(!root.join("agent/demo").exists());
        assert_eq!(
            fs::read_to_string(root.join("agent/market/demo/SKILL.md")).unwrap(),
            "original version"
        );
    }
    #[test]
    fn preview_detects_changes_in_nested_files_and_library() {
        let (root, cfg) = fixture("link-preview");
        let p = preview(&cfg, "fixture", "demo").unwrap();
        fs::write(root.join("agent/demo/nested/data.bin"), [9, 8, 7, 6]).unwrap();
        assert!(apply(&cfg, "fixture", "demo", &p.fingerprint)
            .unwrap_err()
            .contains("内容发生变化"));
        let p = preview(&cfg, "fixture", "demo").unwrap();
        fs::write(root.join("library/demo/SKILL.md"), "updated library").unwrap();
        assert!(apply(&cfg, "fixture", "demo", &p.fingerprint).is_err());
        assert!(!root.join("agent/.agentskillhub-backups").exists());
        assert_eq!(
            fs::read_to_string(root.join("agent/demo/SKILL.md")).unwrap(),
            "original version"
        );
    }
    #[test]
    fn conversion_keeps_original_bytes_and_restores_without_overwriting_foreign_content() {
        let (root, cfg) = fixture("link-restore");
        let p = preview(&cfg, "fixture", "demo").unwrap();
        let record = apply(&cfg, "fixture", "demo", &p.fingerprint).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("agent/demo/SKILL.md")).unwrap(),
            "library version"
        );
        assert_eq!(
            fs::read(Path::new(&record.backup_path).join("nested/data.bin")).unwrap(),
            [0, 1, 2, 255]
        );
        assert!(preview(&cfg, "fixture", "demo").is_err());
        deploy::remove_junction(&root.join("agent/demo")).unwrap();
        fs::create_dir(root.join("agent/demo")).unwrap();
        fs::write(root.join("agent/demo/foreign"), "keep").unwrap();
        assert!(restore(&cfg, &record).unwrap_err().contains("占用"));
        // 夹具中的外部占位移走后，可从停用状态恢复。
        fs::rename(root.join("agent/demo"), root.join("foreign")).unwrap();
        restore(&cfg, &record).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("agent/demo/SKILL.md")).unwrap(),
            "original version"
        );
        assert!(!Path::new(&record.backup_path).exists());
        assert_eq!(
            fs::read_to_string(root.join("foreign/foreign")).unwrap(),
            "keep"
        );
    }
    #[test]
    fn changed_backups_and_retargeted_links_are_protected() {
        let (root, cfg) = fixture("link-protect");
        let p = preview(&cfg, "fixture", "demo").unwrap();
        let record = apply(&cfg, "fixture", "demo", &p.fingerprint).unwrap();
        fs::write(
            Path::new(&record.backup_path).join("SKILL.md"),
            "externally edited",
        )
        .unwrap();
        assert!(restore(&cfg, &record)
            .unwrap_err()
            .contains("备份已被外部修改"));
        fs::write(
            Path::new(&record.backup_path).join("SKILL.md"),
            "original version",
        )
        .unwrap();
        deploy::remove_junction(&root.join("agent/demo")).unwrap();
        deploy::create_junction(&root.join("agent/demo"), &root.join("library")).unwrap();
        assert!(restore(&cfg, &record).is_err());
        assert!(root.join("library/demo/SKILL.md").is_file());
    }
}
