//! 即时分配只操作请求中的技能链接；外部目录及无关的待部署配置始终保留。
use super::{deploy, scan};
use crate::models::*;
use std::{
    collections::HashMap,
    fs,
    path::{Component, Path},
};

pub(super) fn skill_target(root: &str, key: &str) -> Result<std::path::PathBuf, String> {
    if !Path::new(root).is_absolute() {
        return Err("技能库必须使用绝对路径".into());
    }
    if key.is_empty()
        || key.contains('\\')
        || key.contains(':')
        || key
            .split('/')
            .any(|s| s.is_empty() || s.starts_with(['.', '_']))
        || Path::new(key)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err("非法技能标识".into());
    }
    let target =
        scan::resolve_skill_target(root, key).ok_or_else(|| format!("技能已不存在：{key}"))?;
    if !target.join("SKILL.md").is_file() {
        return Err(format!("不是标准技能：{key}"));
    }
    Ok(target)
}

/// 先检查完整批次，遇到冲突时整个请求都不落盘。
pub fn preview(cfg: &AppConfig, changes: &[SkillAssignment]) -> Result<Vec<DeployAction>, String> {
    if changes.is_empty() || changes.len() > 4096 {
        return Err("请选择 1–4096 项分配变更".into());
    }
    let mut paths = HashMap::new();
    let mut actions = Vec::new();
    for change in changes {
        let agent = cfg
            .agents
            .iter()
            .find(|a| a.id == change.agent_id)
            .ok_or("Agent 不存在")?;
        let target = skill_target(&cfg.skills_root, &change.skill_key)?;
        let dir = Path::new(&agent.skills_dir);
        if !dir.is_absolute() {
            return Err("Agent 技能目录必须是绝对路径".into());
        }
        // 根目录别名可把两个 Agent 指到同一位置；也不能经旧整目录链接改写技能源。
        if dir
            .ancestors()
            .any(|p| deploy::same_target(p, Path::new(&cfg.skills_root)))
            || deploy::is_junction(dir)
        {
            return Err(format!(
                "{} 的技能目录是整目录链接或技能库本身，请先在按 Agent 视图迁移或配置独立目录",
                agent.name
            ));
        }
        let link = dir.join(target.file_name().ok_or("技能目录名为空")?);
        let resolved_parent = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        let path_key = deploy::normalize(&resolved_parent.join(target.file_name().unwrap()));
        let requested = (deploy::normalize(&target), change.enabled);
        if let Some(previous) = paths.insert(path_key, requested.clone()) {
            if previous != requested {
                return Err("多个 Agent 共用同一技能目录，分配要求冲突，请先设置独立目录".into());
            }
            continue;
        }
        let metadata = match fs::symlink_metadata(&link) {
            Ok(md) => Some(md),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(format!("无法检查 {}：{e}", link.display())),
        };
        let ours = metadata
            .as_ref()
            .is_some_and(|md| md.file_type().is_symlink())
            && deploy::junction_target(&link).is_some_and(|t| deploy::same_target(&t, &target));
        if metadata.is_some() && !ours {
            return Err(format!(
                "{} 中「{}」由其他工具或用户管理，请先查看 Agent 自有技能；不会覆盖或删除",
                agent.name, change.skill_key
            ));
        }
        if change.enabled == ours {
            continue;
        }
        actions.push(DeployAction {
            kind: if change.enabled {
                "create_link"
            } else {
                "remove_link"
            }
            .into(),
            skill_key: change.skill_key.clone(),
            link_path: link.to_string_lossy().into_owned(),
            target: target.to_string_lossy().into_owned(),
            ok: true,
            message: format!(
                "{} · {}",
                agent.name,
                if change.enabled { "启用" } else { "停用" }
            ),
        });
    }
    Ok(actions)
}

fn run_action(action: &DeployAction, reverse: bool) -> Result<(), String> {
    let create = (action.kind == "create_link") != reverse;
    let link = Path::new(&action.link_path);
    let target = Path::new(&action.target);
    if create {
        deploy::create_junction(link, target)
    } else {
        if !deploy::junction_target(link).is_some_and(|t| deploy::same_target(&t, target)) {
            return Err(format!("链接已被外部修改：{}", link.display()));
        }
        deploy::remove_junction(link)
    }
}

pub fn undo(actions: &[DeployAction]) -> Result<(), String> {
    let errors: Vec<_> = actions
        .iter()
        .rev()
        .filter_map(|a| run_action(a, true).err())
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("；"))
    }
}

pub fn apply(cfg: &AppConfig, changes: &[SkillAssignment]) -> Result<Vec<DeployAction>, String> {
    let actions = preview(cfg, changes)?;
    for (index, action) in actions.iter().enumerate() {
        if let Err(error) = run_action(action, false) {
            return Err(match undo(&actions[..index]) {
                Ok(()) => format!("分配失败，已撤销本批变更：{error}"),
                Err(rollback) => {
                    format!("分配失败：{error}；部分变更未能撤销：{rollback}，请刷新检查")
                }
            });
        }
    }
    Ok(actions)
}

pub fn update_config(cfg: &mut AppConfig, changes: &[SkillAssignment]) {
    for change in changes {
        let enabled = &mut cfg
            .agent_configs
            .entry(change.agent_id.clone())
            .or_default()
            .enabled_skills;
        enabled.retain(|key| key != &change.skill_key);
        if change.enabled {
            enabled.push(change.skill_key.clone());
        }
    }
}

/// 用户安装的实体目录和库外链接，复用标准技能扫描，按实际 Agent 保留来源。
pub fn owned_skills(cfg: &AppConfig, library: &[SkillInfo]) -> Vec<AgentOwnedSkill> {
    fn identity(path: &Path) -> std::path::PathBuf {
        deploy::normalize(&fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
    }
    let library_paths: std::collections::HashSet<_> = library
        .iter()
        .map(|s| identity(Path::new(&s.path)))
        .collect();
    let library_names: HashMap<_, _> = library
        .iter()
        .map(|s| (s.key.rsplit('/').next().unwrap_or(&s.key), &s.key))
        .collect();
    let mut result = Vec::new();
    for agent in &cfg.agents {
        scan::visit_skills(&agent.skills_dir, |path, name, category, key| {
            // 先按实际路径排除已部署技能，避免每个 Agent 重读整份库的图标和详情。
            if library_paths.contains(&identity(&path)) {
                return;
            }
            let library_key = library_names
                .get(key.rsplit('/').next().unwrap_or(&key))
                .map(|key| (*key).clone());
            let skill = scan::make_skill(path, name, category, key);
            result.push(AgentOwnedSkill {
                agent_id: agent.id.clone(),
                agent_name: agent.name.clone(),
                linked: deploy::is_junction(Path::new(&skill.path)),
                skill,
                library_key,
            });
        });
    }
    result.sort_by(|a, b| (&a.skill.name, &a.agent_name).cmp(&(&b.skill.name, &b.agent_name)));
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(tag: &str) -> (std::path::PathBuf, AppConfig, SkillAssignment) {
        let tmp = crate::commands::test_scratch(tag);
        fs::create_dir_all(tmp.join("library/demo")).unwrap();
        fs::create_dir_all(tmp.join("agent")).unwrap();
        fs::write(
            tmp.join("library/demo/SKILL.md"),
            "---\nname: demo\ndescription: test skill\n---\n",
        )
        .unwrap();
        let mut cfg = AppConfig::default();
        cfg.skills_root = tmp.join("library").to_string_lossy().into_owned();
        cfg.agents.push(AgentDef {
            id: "fixture".into(),
            name: "Fixture".into(),
            skills_dir: tmp.join("agent").to_string_lossy().into_owned(),
            builtin: false,
        });
        let change = SkillAssignment {
            agent_id: "fixture".into(),
            skill_key: "demo".into(),
            enabled: true,
        };
        (tmp, cfg, change)
    }
    #[test]
    fn preview_is_read_only_and_external_folders_are_preserved() {
        let (tmp, cfg, change) = fixture("assign_external");
        assert_eq!(preview(&cfg, &[change.clone()]).unwrap().len(), 1);
        assert!(!tmp.join("agent/demo").exists());
        fs::create_dir_all(tmp.join("agent/demo")).unwrap();
        fs::write(tmp.join("agent/demo/private.txt"), "keep").unwrap();
        assert!(apply(&cfg, &[change.clone()]).is_err());
        assert!(apply(
            &cfg,
            &[SkillAssignment {
                enabled: false,
                ..change
            }]
        )
        .is_err());
        assert_eq!(
            fs::read_to_string(tmp.join("agent/demo/private.txt")).unwrap(),
            "keep"
        );
        fs::remove_dir_all(tmp).unwrap();
    }
    #[test]
    fn rejects_traversal_and_root_as_destination() {
        let (tmp, mut cfg, mut change) = fixture("assign_paths");
        change.skill_key = "../demo".into();
        assert!(preview(&cfg, &[change.clone()]).is_err());
        change.skill_key = "demo".into();
        cfg.agents[0].skills_dir = cfg.skills_root.clone();
        assert!(preview(&cfg, &[change]).is_err());
        fs::remove_dir_all(tmp).unwrap();
    }
    #[test]
    fn detects_regular_agent_skills_and_keeps_name_conflicts_separate() {
        let (tmp, cfg, _) = fixture("assign_owned");
        fs::create_dir_all(tmp.join("agent/demo")).unwrap();
        fs::write(
            tmp.join("agent/demo/SKILL.md"),
            "---\nname: demo\ndescription: different skill\n---\n",
        )
        .unwrap();
        let owned = owned_skills(&cfg, &scan::scan_skills(&cfg.skills_root));
        assert_eq!(owned.len(), 1);
        assert!(!owned[0].linked);
        assert_eq!(owned[0].library_key.as_deref(), Some("demo"));
        fs::remove_dir_all(tmp).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn deeply_wrapped_skill_can_be_assigned_and_undone() {
        let (tmp, cfg, mut change) = fixture("assign_deep");
        let key = "外层/内层/开发/demo";
        let target = tmp.join("library").join(key);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::rename(tmp.join("library/demo"), &target).unwrap();
        change.skill_key = key.into();
        assert_eq!(preview(&cfg, &[change.clone()]).unwrap().len(), 1);
        let actions = apply(&cfg, &[change]).unwrap();
        assert!(tmp.join("agent/demo/SKILL.md").is_file());
        assert_eq!(deploy::assigned_skills(&cfg.agents[0], &cfg.skills_root), [key]);
        undo(&actions).unwrap();
        assert!(target.join("SKILL.md").is_file());
        fs::remove_dir_all(tmp).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn instant_toggle_is_scoped_and_can_be_undone() {
        let (tmp, mut cfg, change) = fixture("assign_instant");
        cfg.agent_configs
            .entry("fixture".into())
            .or_default()
            .enabled_skills
            .push("unrelated-pending".into());
        let actions = apply(&cfg, &[change.clone()]).unwrap();
        assert_eq!(
            deploy::assigned_skills(&cfg.agents[0], &cfg.skills_root),
            vec!["demo"]
        );
        assert!(!tmp.join("agent/unrelated-pending").exists());
        undo(&actions).unwrap();
        assert!(!tmp.join("agent/demo").exists());
        apply(&cfg, &[change.clone()]).unwrap();
        apply(
            &cfg,
            &[SkillAssignment {
                enabled: false,
                ..change
            }],
        )
        .unwrap();
        assert!(tmp.join("library/demo/SKILL.md").is_file());
        fs::remove_dir_all(tmp).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn batch_checks_conflicts_before_creating_any_links() {
        let (tmp, mut cfg, change) = fixture("assign_batch");
        cfg.agents.push(AgentDef {
            id: "second".into(),
            name: "Second".into(),
            skills_dir: tmp.join("second").to_string_lossy().into_owned(),
            builtin: false,
        });
        fs::create_dir_all(tmp.join("second/demo")).unwrap();
        assert!(apply(
            &cfg,
            &[
                change.clone(),
                SkillAssignment {
                    agent_id: "second".into(),
                    ..change
                }
            ]
        )
        .is_err());
        assert!(!tmp.join("agent/demo").exists());
        fs::remove_dir_all(tmp).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn shared_destination_conflicts_and_external_retargeting_are_preserved() {
        let (tmp, mut cfg, change) = fixture("assign_shared");
        cfg.agents.push(AgentDef {
            id: "alias".into(),
            name: "Alias".into(),
            ..cfg.agents[0].clone()
        });
        assert!(preview(
            &cfg,
            &[
                change.clone(),
                SkillAssignment {
                    agent_id: "alias".into(),
                    enabled: false,
                    ..change.clone()
                }
            ]
        )
        .is_err());
        let actions = apply(&cfg, &[change.clone()]).unwrap();
        deploy::remove_junction(Path::new(&actions[0].link_path)).unwrap();
        fs::create_dir_all(tmp.join("external")).unwrap();
        deploy::create_junction(Path::new(&actions[0].link_path), &tmp.join("external")).unwrap();
        assert!(undo(&actions).is_err());
        assert!(deploy::same_target(
            Path::new(&actions[0].link_path),
            &tmp.join("external")
        ));
        deploy::remove_junction(Path::new(&actions[0].link_path)).unwrap();
        // 根目录重定向到技能库时，拒绝经链接操作源文件。
        fs::remove_dir(tmp.join("agent")).unwrap();
        deploy::create_junction(&tmp.join("agent"), &tmp.join("library")).unwrap();
        assert!(preview(&cfg, &[change]).is_err());
        deploy::remove_junction(&tmp.join("agent")).unwrap();
        fs::remove_dir_all(tmp).unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn junction_paths_preserve_spaces_forward_slashes_and_shell_characters() {
        let tmp = crate::commands::test_scratch("assign_quoted_paths");
        let target = tmp.join("source & %PATH% !");
        let link = tmp.join("link & %PATH% !");
        fs::create_dir_all(&target).unwrap();
        let forward_link = link.to_string_lossy().replace('\\', "/");
        let forward_target = target.to_string_lossy().replace('\\', "/");
        deploy::create_junction(Path::new(&forward_link), Path::new(&forward_target)).unwrap();
        assert!(deploy::same_target(&link, &target));
        deploy::remove_junction(&link).unwrap();
        fs::remove_dir_all(tmp).unwrap();
    }
}
