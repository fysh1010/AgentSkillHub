//! MCP 路径与核查依据独立于技能目录。未适配的产品只提供配置指南。
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::OnceLock};

#[derive(Clone, Serialize, Deserialize)]
pub struct Guide {
    pub agent_id: String,
    pub path: String,
    pub format: String,
    pub docs_url: String,
    pub notes: String,
    pub checked_at: String,
    pub status: String,
}
pub fn guides() -> &'static Vec<Guide> {
    static GUIDES: OnceLock<Vec<Guide>> = OnceLock::new();
    GUIDES.get_or_init(|| {
        serde_json::from_str(include_str!("../../resources/mcp-agents.json"))
            .expect("bundled MCP catalog must be valid")
    })
}
pub fn guide(id: &str) -> Option<&'static Guide> {
    guides().iter().find(|g| g.agent_id == id)
}
pub fn reload_notice(id: &str) -> &'static str {
    match id {
        "qoder" | "qoder-cn" | "qoder-cli" => "配置立即写入磁盘；Qoder CLI 官方要求重启后加载 MCP。IDE 请重新加载连接或重启。",
        "claude" => "配置立即写入磁盘；请在 Claude Code 中用 /mcp 检查连接，必要时重新连接或重开会话。",
        "codex" => "配置立即写入磁盘；请在 Codex 的 MCP 设置中重新连接，必要时重开会话。授权仍在 Codex 内完成。",
        "workbuddy" => "配置立即写入磁盘；请在 WorkBuddy 的连接器设置中确认信任并重连，必要时重开任务。",
        _ => "配置立即写入磁盘；是否自动重载由目标 Agent 决定。请在原工具中确认连接；必要时重新加载或重启。",
    }
}
pub fn resolve(id: &str) -> Option<(PathBuf, String)> {
    let g = guide(id)?;
    if g.format.is_empty() {
        return None;
    }
    let home = super::user_home()?;
    let xdg = super::env_dir("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
    let path = if let Some(tail) = g.path.strip_prefix("~/") {
        if let Some(tail) = tail.strip_prefix(".config/") {
            xdg.join(tail)
        } else {
            home.join(tail)
        }
    } else {
        let rest = g.path.strip_prefix('$')?;
        let (env, tail) = rest.split_once('/')?;
        let default = match env {
            "CODEX_HOME" => home.join(".codex"),
            "WORKBUDDY_CONFIG_DIR" => home.join(".workbuddy"),
            "COPILOT_HOME" => home.join(".copilot"),
            "GROK_HOME" => home.join(".grok"),
            _ => super::env_dir(env)?,
        };
        super::env_dir(env).unwrap_or(default).join(tail)
    };
    let mut format = g.format.clone();
    let mut candidates = vec![path];
    match id {
        "qoder" | "qoder-cli" => {
            if let Some(dir) = super::env_dir("QODER_CONFIG_DIR") {
                candidates = vec![dir.join("settings.json")];
            }
        }
        "claude" => {
            if let Some(dir) = super::env_dir("CLAUDE_CONFIG_DIR") {
                candidates = vec![dir.join(".claude.json")];
            }
        }
        "cline" => {
            if let Some(file) = super::env_dir("CLINE_MCP_SETTINGS_PATH") {
                candidates = vec![file];
            } else if let Some(app) = super::env_dir("APPDATA") {
                candidates.push(app.join("Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json"));
            }
        }
        "kimi-code" => {
            if let Some(dir) =
                super::env_dir("KIMI_CODE_HOME").or_else(|| super::env_dir("KIMI_SHARE_DIR"))
            {
                candidates = vec![dir.join("mcp.json")];
            } else {
                candidates.push(home.join(".kimi/mcp.json"));
            }
        }
        "codebuddy" => {
            candidates.push(home.join(".codebuddy/mcp.json"));
            candidates.push(
                super::env_dir("CODEBUDDY_CONFIG_DIR")
                    .unwrap_or(home.clone())
                    .join(".codebuddy.json"),
            );
        }
        "opencode" => {
            if let Some(file) = super::env_dir("OPENCODE_CONFIG") {
                candidates = vec![file];
            } else {
                candidates.push(xdg.join("opencode/opencode.jsonc"));
            }
        }
        "claude-desktop" => {
            if let Some(app) = super::env_dir("LOCALAPPDATA") {
                candidates.push(app.join("Claude/claude_desktop_config.json"));
            }
        }
        _ => {}
    }
    let path = candidates
        .iter()
        .find(|p| p.is_file())
        .unwrap_or(&candidates[0])
        .clone();
    // 同名 Cline CLI / IDE 文件有不同 schema：有 transport 对象时用 CLI 适配。
    if id == "cline" && path.is_file() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if !text.contains("\"transport\"")
                && (path.to_string_lossy().contains("globalStorage")
                    || text.contains("\"command\"")
                    || text.contains("\"url\""))
            {
                format = "json".into();
            }
        }
    }
    Some((path, format))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_covers_every_agent_and_keeps_unverified_schemas_read_only() {
        for agent in super::super::known_agents::KNOWN_AGENTS {
            if ["agents", "config-agents"].contains(&agent.id) {
                continue;
            }
            assert!(
                guide(agent.id).is_some(),
                "missing MCP research for {}",
                agent.id
            );
        }
        let unique: std::collections::BTreeSet<_> = guides().iter().map(|g| &g.agent_id).collect();
        assert_eq!(unique.len(), guides().len());
        assert!(resolve("hermes-agent").is_none());
        assert!(resolve("autohand-code").is_none());
    }
}
