//! Doctor 体检：扫描所有 Agent 技能目录与技能库，找出失效链接、
//! 配置引用失效技能、待重新部署（漂移）、孤儿实体目录、库内异常技能，
//! 并支持一键修复。

use crate::models::*;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// 生成稳定的问题 ID："kind|agent_id|path"（路径含 \ 和 : 但不含 |，安全）
fn issue_id(kind: &str, agent_id: &str, path: &str) -> String {
    format!("{kind}|{agent_id}|{path}")
}

/// kind 排序权重（越靠前越紧急）
fn kind_rank(kind: &str) -> u8 {
    match kind {
        "broken_link" => 0,
        "stale_config" => 1,
        "wrong_link" => 2,
        "drift" => 3,
        "orphan_dir" => 4,
        _ => 5,
    }
}

/// 体检扫描（纯读，不改任何状态）
pub fn doctor_scan(cfg: &AppConfig) -> Vec<DoctorIssue> {
    let mut issues: Vec<DoctorIssue> = Vec::new();
    let root = cfg.skills_root.clone();
    let skills = crate::commands::scan::scan_skills(&root);
    let skill_keys: HashSet<String> = skills.iter().map(|s| s.key.clone()).collect();

    for agent in &cfg.agents {
        let agent_name = agent.name.clone();
        let enabled: Vec<String> = cfg
            .agent_configs
            .get(&agent.id)
            .map(|c| c.enabled_skills.clone())
            .unwrap_or_default();

        // 1. 配置里引用了库中不存在的技能
        for key in &enabled {
            if !skill_keys.contains(key)
                && crate::commands::scan::resolve_skill_target(&root, key).is_none()
            {
                issues.push(DoctorIssue {
                    id: issue_id("stale_config", &agent.id, &format!("config:{key}")),
                    kind: "stale_config".into(),
                    agent_id: agent.id.clone(),
                    agent_name: agent_name.clone(),
                    path: String::new(),
                    skill_key: key.clone(),
                    detail: format!("配置引用的技能「{key}」在技能库中不存在"),
                    fixable: true,
                });
            }
        }

        let skills_dir = Path::new(&agent.skills_dir);
        if !skills_dir.is_dir() {
            continue;
        }
        let links = crate::commands::deploy::scan_agent_links(skills_dir);

        // 2. 整目录 junction：目标失效才算 broken
        if let Some(pt) = &links.parent_link {
            if !pt.exists() {
                issues.push(DoctorIssue {
                    id: issue_id("broken_link", &agent.id, &agent.skills_dir),
                    kind: "broken_link".into(),
                    agent_id: agent.id.clone(),
                    agent_name: agent_name.clone(),
                    path: agent.skills_dir.clone(),
                    skill_key: "_parent".into(),
                    detail: "整目录链接的目标已不存在".into(),
                    fixable: true,
                });
            }
        }

        let mut reported_paths: HashSet<String> = HashSet::new();

        // 3. 逐条目检查
        for (name, kind, tgt) in &links.entries {
            let link_path = skills_dir.join(name).to_string_lossy().to_string();
            if kind == "link" {
                match tgt {
                    None => {
                        issues.push(DoctorIssue {
                            id: issue_id("broken_link", &agent.id, &link_path),
                            kind: "broken_link".into(),
                            agent_id: agent.id.clone(),
                            agent_name: agent_name.clone(),
                            path: link_path.clone(),
                            skill_key: name.clone(),
                            detail: "链接无法解析目标".into(),
                            fixable: true,
                        });
                        reported_paths.insert(link_path);
                    }
                    Some(t) => {
                        if !t.exists() {
                            issues.push(DoctorIssue {
                                id: issue_id("broken_link", &agent.id, &link_path),
                                kind: "broken_link".into(),
                                agent_id: agent.id.clone(),
                                agent_name: agent_name.clone(),
                                path: link_path.clone(),
                                skill_key: name.clone(),
                                detail: format!("链接目标已不存在: {}", t.display()),
                                fixable: true,
                            });
                            reported_paths.insert(link_path);
                        } else if crate::commands::deploy::within_root(&root, t) {
                            // 指向库内但不在该 Agent 的启用列表 → 需要重新部署对齐
                            let key = crate::commands::deploy::find_key_by_path(&skills, t);
                            let enabled_hit = key
                                .as_ref()
                                .map(|k| enabled.contains(k))
                                .unwrap_or(false);
                            if !enabled_hit {
                                issues.push(DoctorIssue {
                                    id: issue_id("wrong_link", &agent.id, &link_path),
                                    kind: "wrong_link".into(),
                                    agent_id: agent.id.clone(),
                                    agent_name: agent_name.clone(),
                                    path: link_path.clone(),
                                    skill_key: key.unwrap_or_else(|| name.clone()),
                                    detail: "链接指向技能库，但该技能未在此 Agent 启用".into(),
                                    fixable: true,
                                });
                                reported_paths.insert(link_path);
                            }
                        }
                        // 指向库外的链接：与部署策略一致（保留），不报
                    }
                }
            } else {
                // 实体目录：提示可收编，不自动修
                issues.push(DoctorIssue {
                    id: issue_id("orphan_dir", &agent.id, &link_path),
                    kind: "orphan_dir".into(),
                    agent_id: agent.id.clone(),
                    agent_name: agent_name.clone(),
                    path: link_path,
                    skill_key: name.clone(),
                    detail: "实体目录（非本工具创建的链接），可在总览页收编进技能库或忽略".into(),
                    fixable: false,
                });
            }
        }

        // 4. 部署漂移：有可执行差异项 → 汇总为一条（重新部署即可修复）
        let drift: Vec<DiffItem> = crate::commands::deploy::compute_diff(agent, cfg)
            .into_iter()
            .filter(|d| {
                matches!(
                    d.kind.as_str(),
                    "to_create" | "to_remove" | "to_replace" | "expand_parent_link"
                ) && !reported_paths.contains(&d.link_path)
            })
            .collect();
        if !drift.is_empty() {
            let creates = drift.iter().filter(|d| d.kind == "to_create").count();
            let removes = drift.iter().filter(|d| d.kind == "to_remove").count();
            let replaces = drift.iter().filter(|d| d.kind == "to_replace").count();
            let mut parts = Vec::new();
            if creates > 0 {
                parts.push(format!("{creates} 个待创建"));
            }
            if removes > 0 {
                parts.push(format!("{removes} 个待移除"));
            }
            if replaces > 0 {
                parts.push(format!("{replaces} 个待修正"));
            }
            issues.push(DoctorIssue {
                id: issue_id("drift", &agent.id, &agent.skills_dir),
                kind: "drift".into(),
                agent_id: agent.id.clone(),
                agent_name: agent_name.clone(),
                path: agent.skills_dir.clone(),
                skill_key: String::new(),
                detail: format!("实际链接与配置不一致（{}），重新部署即可对齐", parts.join("、")),
                fixable: true,
            });
        }
    }

    // 5. 库级异常：缺 SKILL.md 的顶层目录 / frontmatter 缺 name 的技能
    if let Ok(rd) = fs::read_dir(&root) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name.starts_with('_') || name == "node_modules" {
                continue;
            }
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            if !p.join("SKILL.md").is_file() {
                // 可能是旧两层结构的分类目录：其下有含 SKILL.md 的子目录则跳过
                let has_skill = fs::read_dir(&p)
                    .map(|sub| {
                        sub.flatten()
                            .any(|se| se.path().join("SKILL.md").is_file())
                    })
                    .unwrap_or(false);
                if !has_skill {
                    issues.push(DoctorIssue {
                        id: issue_id("unhealthy_skill", "", &p.to_string_lossy()),
                        kind: "unhealthy_skill".into(),
                        agent_id: String::new(),
                        agent_name: String::new(),
                        path: p.to_string_lossy().to_string(),
                        skill_key: name.clone(),
                        detail: "技能库内目录缺少 SKILL.md，不会被识别为技能".into(),
                        fixable: false,
                    });
                }
            }
        }
    }
    for s in skills.iter().filter(|s| !s.healthy) {
        issues.push(DoctorIssue {
            id: issue_id("unhealthy_skill", "", &s.path),
            kind: "unhealthy_skill".into(),
            agent_id: String::new(),
            agent_name: String::new(),
            path: s.path.clone(),
            skill_key: s.key.clone(),
            detail: "SKILL.md 的 frontmatter 缺少 name 字段".into(),
            fixable: false,
        });
    }

    issues.sort_by(|a, b| {
        kind_rank(&a.kind)
            .cmp(&kind_rank(&b.kind))
            .then(a.agent_id.cmp(&b.agent_id))
            .then(a.path.cmp(&b.path))
    });
    issues
}

/// 执行修复。顺序：先清 stale_config（配置），再删 broken_link（链接），
/// 最后对 wrong_link / drift 涉及的 Agent 重新部署对齐。
pub fn doctor_fix(
    cfg: &mut AppConfig,
    ids: &[String],
) -> (DoctorFixResult, Vec<DeployAction>) {
    let mut result = DoctorFixResult {
        fixed: 0,
        failed: 0,
        messages: Vec::new(),
    };
    let mut actions: Vec<DeployAction> = Vec::new();
    let mut deploy_agents: Vec<String> = Vec::new();

    let parse_id = |id: &str| -> (String, String, String) {
        let mut it = id.splitn(3, '|');
        (
            it.next().unwrap_or("").to_string(),
            it.next().unwrap_or("").to_string(),
            it.next().unwrap_or("").to_string(),
        )
    };

    for id in ids {
        let (kind, agent_id, path) = parse_id(id);
        match kind.as_str() {
            "stale_config" => {
                // path 形如 "config:<key>"
                if let Some(key) = path.strip_prefix("config:") {
                    if let Some(ac) = cfg.agent_configs.get_mut(&agent_id) {
                        let before = ac.enabled_skills.len();
                        ac.enabled_skills.retain(|k| k != key);
                        if ac.enabled_skills.len() < before {
                            result.fixed += 1;
                            result.messages.push(format!("已从 {agent_id} 配置移除失效技能「{key}」"));
                        }
                    }
                }
            }
            "broken_link" => {
                // 整目录链接失效（skills_dir 本身）时，path 即 skills_dir——
                // 目标已不存在，删除死链接后下次部署会重建真实目录
                if path.is_empty() {
                    continue;
                }
                match crate::commands::deploy::remove_junction(Path::new(&path)) {
                    Ok(()) => {
                        result.fixed += 1;
                        result.messages.push(format!("已移除失效链接 {path}"));
                        actions.push(DeployAction {
                            kind: "remove_link".into(),
                            skill_key: String::new(),
                            link_path: path.clone(),
                            target: String::new(),
                            ok: true,
                            message: "体检修复：移除失效链接".into(),
                        });
                    }
                    Err(e) => {
                        result.failed += 1;
                        result.messages.push(format!("移除失效链接失败 {path}: {e}"));
                        actions.push(DeployAction {
                            kind: "remove_link".into(),
                            skill_key: String::new(),
                            link_path: path.clone(),
                            target: String::new(),
                            ok: false,
                            message: e,
                        });
                    }
                }
            }
            "wrong_link" | "drift" => {
                if !deploy_agents.contains(&agent_id) {
                    deploy_agents.push(agent_id.clone());
                }
            }
            other => {
                result.messages.push(format!("「{other}」类型问题不支持自动修复，请手动处理"));
            }
        }
    }

    // 重新部署对齐（stale_config 已先清理，deploy 会基于新配置计算差异）
    for agent_id in deploy_agents {
        let Some(agent) = cfg.agents.iter().find(|a| a.id == agent_id).cloned() else {
            continue;
        };
        let r = crate::commands::deploy::deploy(&agent, cfg, false);
        result.fixed += r.actions.iter().filter(|a| a.ok).count() as u32;
        result.failed += r.actions.iter().filter(|a| !a.ok).count() as u32;
        result.messages.push(format!("已重新部署 {agent_id}：{}", r.summary));
        actions.extend(r.actions);
    }

    (result, actions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn tmp(tag: &str) -> PathBuf {
        let d = crate::commands::test_scratch(&format!("doctor_{tag}"));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_cfg(root: &Path, agent_dir: &Path) -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.skills_root = root.to_string_lossy().to_string();
        cfg.agents.push(AgentDef {
            id: "t".into(),
            name: "测试".into(),
            skills_dir: agent_dir.to_string_lossy().to_string(),
            builtin: false,
        });
        let mut ac = AgentConfig::default();
        ac.enabled_skills = vec!["gh-cli".into(), "ghost-skill".into()];
        let mut m = HashMap::new();
        m.insert("t".to_string(), ac);
        cfg.agent_configs = m;
        cfg
    }

    #[test]
    fn detects_broken_stale_and_ignores_healthy() {
        let root = tmp("root");
        let agent_dir = tmp("agent");
        // 健康技能
        let skill = root.join("gh-cli");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            "---\nname: gh-cli\ndescription: test\n---\nbody",
        )
        .unwrap();
        // 健康链接
        crate::commands::deploy::create_junction(&agent_dir.join("gh-cli"), &skill).unwrap();
        // 失效链接（目标不存在）
        crate::commands::deploy::create_junction(&agent_dir.join("dead"), &root.join("nope")).unwrap();

        let cfg = make_cfg(&root, &agent_dir);
        let issues = doctor_scan(&cfg);

        assert!(issues.iter().any(|i| i.kind == "broken_link" && i.skill_key == "dead"), "应检出失效链接");
        assert!(issues.iter().any(|i| i.kind == "stale_config" && i.skill_key == "ghost-skill"), "应检出配置引用失效");
        assert!(!issues.iter().any(|i| i.skill_key == "gh-cli" && i.kind != "drift"), "健康链接不应报问题");

        // 修复：清 stale + 删 broken
        let ids: Vec<String> = issues
            .iter()
            .filter(|i| i.fixable && (i.kind == "stale_config" || i.kind == "broken_link"))
            .map(|i| i.id.clone())
            .collect();
        let mut cfg2 = make_cfg(&root, &agent_dir);
        let (res, _) = doctor_fix(&mut cfg2, &ids);
        assert_eq!(res.failed, 0);
        assert!(!cfg2.agent_configs["t"].enabled_skills.contains(&"ghost-skill".to_string()), "失效配置应被清理");
        assert!(!crate::commands::deploy::is_junction(&agent_dir.join("dead")), "失效链接应被删除");

        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&agent_dir);
    }

    #[test]
    fn orphan_dir_not_touchable() {
        let root = tmp("orphan_root");
        let agent_dir = tmp("orphan_agent");
        fs::create_dir_all(agent_dir.join("manual-skill")).unwrap();
        let mut cfg = make_cfg(&root, &agent_dir);
        cfg.agent_configs.get_mut("t").unwrap().enabled_skills.clear();
        let issues = doctor_scan(&cfg);
        let orphan = issues.iter().find(|i| i.kind == "orphan_dir").expect("应检出孤儿目录");
        assert!(!orphan.fixable);
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&agent_dir);
    }

    #[test]
    fn unhealthy_library_skill_flagged() {
        let root = tmp("unhealthy");
        fs::create_dir_all(root.join("no-skill-md")).unwrap();
        let mut cfg = AppConfig::default();
        cfg.skills_root = root.to_string_lossy().to_string();
        let issues = doctor_scan(&cfg);
        assert!(issues.iter().any(|i| i.kind == "unhealthy_skill" && i.skill_key == "no-skill-md"));
        let _ = fs::remove_dir_all(&root);
    }
}
