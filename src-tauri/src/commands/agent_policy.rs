//! 安装证据、配置开关及所有 Agent 写入入口共用的权限判断。
use crate::models::*;
use std::path::Path;

pub fn enabled(cfg: &AppConfig, id: &str) -> bool {
    cfg.agent_preferences
        .get(id)
        .map(|p| p.enabled)
        .unwrap_or_else(|| cfg.agents.iter().any(|a| a.id == id))
}

pub fn annotate(cfg: &AppConfig, inventory: &mut MachineInventory) {
    for agent in &mut inventory.agents {
        agent.registered = cfg.agents.iter().any(|a| a.id == agent.id);
        if let Some(preference) = cfg.agent_preferences.get(&agent.id) {
            agent.custom = preference.manual_confirmed;
            if preference.manual_confirmed {
                agent.executable_path = preference.executable_path.clone();
                let program_exists = !preference.executable_path.is_empty()
                    && Path::new(&preference.executable_path).is_file();
                let directory_confirmed = preference.executable_path.is_empty() && agent.dir_exists;
                if program_exists || directory_confirmed {
                    agent.installed = true;
                    agent.confidence = "confirmed".into();
                    agent.evidence.retain(|e| e.kind != "手动确认");
                    agent.evidence.push(DetectionEvidence {
                        kind: "手动确认".into(),
                        path: if program_exists {
                            preference.executable_path.clone()
                        } else {
                            agent.skills_dir.clone()
                        },
                    });
                }
            }
        }
        agent.management_enabled =
            agent.installed && !agent.shared && agent.registered && enabled(cfg, &agent.id);
    }
}

fn installation_exists(agent: &MachineAgent) -> bool {
    (!agent.executable_path.is_empty() && Path::new(&agent.executable_path).is_file())
        || agent.evidence.iter().any(|e| {
            matches!(
                e.kind.as_str(),
                "IDE 扩展" | "安装登记" | "运行进程" | "PATH 命令" | "开始菜单" | "常见安装位置"
            ) && Path::new(&e.path).is_file()
        })
        || (agent.custom
            && agent.executable_path.is_empty()
            && Path::new(&agent.skills_dir).is_dir())
}

pub fn require_agent<'a>(
    cfg: &'a AppConfig,
    inventory: Option<&MachineInventory>,
    id: &str,
) -> Result<&'a AgentDef, String> {
    if !enabled(cfg, id) {
        return Err("该 Agent 已关闭配置，请在设置中开启「允许配置」".into());
    }
    let agent = cfg
        .agents
        .iter()
        .find(|a| a.id == id)
        .ok_or("Agent 尚未加入管理，请先在设置中开启配置")?;
    let detected = inventory
        .ok_or("本机扫描尚未完成，请在设置中重新扫描")?
        .agents
        .iter()
        .find(|a| a.id == id)
        .ok_or("本机未检测到该 Agent")?;
    if !detected.installed || detected.shared || !installation_exists(detected) {
        return Err("该工具尚未确认安装，不能通过本软件配置".into());
    }
    Ok(agent)
}

pub fn active_config(cfg: &AppConfig, inventory: Option<&MachineInventory>) -> AppConfig {
    let mut active = cfg.clone();
    active
        .agents
        .retain(|a| require_agent(cfg, inventory, &a.id).is_ok());
    active
}

pub fn set_enabled(
    cfg: &mut AppConfig,
    inventory: &mut MachineInventory,
    id: &str,
    value: bool,
) -> Result<(), String> {
    let detected = inventory
        .agents
        .iter()
        .find(|a| a.id == id)
        .ok_or("未找到该工具，请重新扫描")?;
    if value && (!detected.installed || detected.shared || !installation_exists(detected)) {
        return Err("仅允许为已安装的 Agent 开启配置；未知工具请先自定义确认路径".into());
    }
    if value && !cfg.agents.iter().any(|a| a.id == id) {
        cfg.agents.push(AgentDef {
            id: detected.id.clone(),
            name: detected.name.clone(),
            skills_dir: detected.skills_dir.clone(),
            builtin: detected.known,
        });
    }
    cfg.agent_preferences.entry(id.into()).or_default().enabled = value;
    cfg.agent_configs.entry(id.into()).or_default();
    annotate(cfg, inventory);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (AppConfig, MachineInventory) {
        static SEQUENCE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let installation = super::super::test_scratch(&format!(
            "policy_program_{}",
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&installation).unwrap();
        let executable = installation.join("codex.exe");
        std::fs::write(&executable, b"fixture").unwrap();
        let mut cfg = AppConfig::default();
        cfg.agents.push(AgentDef {
            id: "codex".into(),
            name: "Codex".into(),
            skills_dir: "C:/test/skills".into(),
            builtin: true,
        });
        cfg.agent_configs.insert(
            "codex".into(),
            AgentConfig {
                enabled_skills: vec!["preserved".into()],
            },
        );
        let inventory = MachineInventory {
            home_dir: String::new(),
            sources: vec![],
            warnings: vec![],
            scanned_at: 0,
            catalog_count: 1,
            agents: vec![MachineAgent {
                id: "codex".into(),
                name: "Codex".into(),
                skills_dir: "C:/test/skills".into(),
                resolved_dir: String::new(),
                path_links: vec![],
                dir_exists: false,
                registered: true,
                known: true,
                shared: false,
                confidence: "confirmed".into(),
                install_dir: String::new(),
                evidence: vec![],
                installed: true,
                management_enabled: false,
                executable_path: executable.to_string_lossy().into_owned(),
                custom: false,
            }],
        };
        (cfg, inventory)
    }
    #[test]
    fn switch_off_blocks_every_write_guard_and_keeps_assignments() {
        let (mut cfg, mut inventory) = fixture();
        assert!(require_agent(&cfg, Some(&inventory), "codex").is_ok());
        set_enabled(&mut cfg, &mut inventory, "codex", false).unwrap();
        assert!(require_agent(&cfg, Some(&inventory), "codex").is_err());
        assert!(active_config(&cfg, Some(&inventory)).agents.is_empty());
        assert_eq!(cfg.agent_configs["codex"].enabled_skills, ["preserved"]);
        annotate(&cfg, &mut inventory);
        assert!(
            !inventory.agents[0].management_enabled,
            "重新扫描不能覆盖关闭状态"
        );
        set_enabled(&mut cfg, &mut inventory, "codex", true).unwrap();
        assert!(inventory.agents[0].management_enabled);
    }
    #[test]
    fn uninstall_after_scan_revokes_write_access() {
        let (cfg, mut inventory) = fixture();
        let program = std::path::PathBuf::from(&inventory.agents[0].executable_path);
        assert!(require_agent(&cfg, Some(&inventory), "codex").is_ok());
        std::fs::remove_file(program).unwrap();
        assert!(require_agent(&cfg, Some(&inventory), "codex").is_err());
        let mut cfg = cfg;
        assert!(set_enabled(&mut cfg, &mut inventory, "codex", true).is_err());
    }

    #[test]
    fn directory_residue_and_unfinished_scan_cannot_be_configured() {
        let (mut cfg, mut inventory) = fixture();
        inventory.agents[0].installed = false;
        inventory.agents[0].dir_exists = true;
        assert!(set_enabled(&mut cfg, &mut inventory, "codex", true).is_err());
        assert!(require_agent(&cfg, Some(&inventory), "codex").is_err());
        assert!(require_agent(&cfg, None, "codex").is_err());
    }
    #[test]
    fn fresh_install_waits_for_user_enable_and_can_be_enabled_once() {
        let (mut cfg, mut inventory) = fixture();
        cfg.agents.clear();
        annotate(&cfg, &mut inventory);
        assert!(!inventory.agents[0].management_enabled);
        set_enabled(&mut cfg, &mut inventory, "codex", true).unwrap();
        set_enabled(&mut cfg, &mut inventory, "codex", true).unwrap();
        assert_eq!(cfg.agents.len(), 1);
        assert!(require_agent(&cfg, Some(&inventory), "codex").is_ok());
    }
    #[test]
    fn shared_directory_is_never_an_installed_agent() {
        let (mut cfg, mut inventory) = fixture();
        inventory.agents[0].shared = true;
        assert!(set_enabled(&mut cfg, &mut inventory, "codex", true).is_err());
        assert!(require_agent(&cfg, Some(&inventory), "codex").is_err());
    }
    #[test]
    fn switch_serialization_and_legacy_defaults_are_compatible() {
        let (mut cfg, mut inventory) = fixture();
        assert!(enabled(&cfg, "codex"));
        set_enabled(&mut cfg, &mut inventory, "codex", false).unwrap();
        let restored: AppConfig =
            serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert!(!enabled(&restored, "codex"));
    }
}
