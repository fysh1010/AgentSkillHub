pub mod delete;
pub mod deploy;
pub mod market;
pub mod scan;

use crate::models::*;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub handle: AppHandle,
}

/// 内置 Agent 默认注册表（路径可在设置中修改）
fn default_agents() -> Vec<AgentDef> {
    vec![
        AgentDef {
            id: "claude".into(),
            name: "Claude Code".into(),
            skills_dir: r"C:\Users\admin\.claude\skills".into(),
            builtin: true,
        },
        AgentDef {
            id: "codex".into(),
            name: "OpenAI Codex".into(),
            skills_dir: r"C:\Users\admin\.codex\skills".into(),
            builtin: true,
        },
        AgentDef {
            id: "zcode".into(),
            name: "Zcode".into(),
            skills_dir: r"C:\Users\admin\.zcode\skills".into(),
            builtin: true,
        },
        AgentDef {
            id: "workbuddy".into(),
            name: "WorkBuddy".into(),
            skills_dir: r"C:\Users\admin\.workbuddy\skills".into(),
            builtin: true,
        },
        AgentDef {
            id: "agents".into(),
            name: "Agents".into(),
            skills_dir: r"C:\Users\admin\.agents\skills".into(),
            builtin: true,
        },
        AgentDef {
            id: "doubao".into(),
            name: "豆包 Doubao".into(),
            skills_dir: r"C:\Users\admin\AppData\Local\Doubao\User Data\Default\.doubao\agent_mode\workspace\.user_skills".into(),
            builtin: true,
        },
    ]
}

fn now_ts() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Windows 上 GUI 进程启动控制台子进程时，系统会为它分配一个新的控制台窗口，
/// 表现为「弹黑框」。这里统一用 CREATE_NO_WINDOW 抑制。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 构造一个静默子进程（不弹控制台窗口）。
/// 所有 curl / tar / python / cmd 调用都应经由此函数创建。
pub fn hidden_command(program: &str) -> std::process::Command {
    #[allow(unused_mut)]
    let mut cmd = std::process::Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// 从磁盘加载配置（首次自动初始化默认值）
pub fn load_config(app: &AppHandle) -> AppConfig {
    let mut cfg = AppConfig::default();
    if let Ok(dir) = app.path().app_config_dir() {
        let file = dir.join("config.json");
        if let Ok(content) = std::fs::read_to_string(&file) {
            if let Ok(parsed) = serde_json::from_str::<AppConfig>(&content) {
                cfg = parsed;
            }
        }
    }
    if cfg.skills_root.is_empty() {
        cfg.skills_root = r"E:\my-skills".into();
    }
    if cfg.agents.is_empty() {
        cfg.agents = default_agents();
    }
    if cfg.market_sources.is_empty() {
        cfg.market_sources = market::default_market_sources();
    } else {
        // 老配置自动补上腾讯 SkillHub 源
        if !cfg.market_sources.iter().any(|s| s.kind == "skillhub") {
            cfg.market_sources.push(MarketSource {
                id: "tencent-skillhub".into(),
                kind: "skillhub".into(),
                name: "腾讯 SkillHub".into(),
                repo: String::new(),
                subpath: String::new(),
            });
        }
        // 移除已下线的内置源（Anthropic 官方技能，用户反馈无用）
        cfg.market_sources.retain(|s| s.id != "github:anthropics/skills");
    }
    // 腾讯源固定排最前（用户偏好：打开市场默认先看腾讯精选）
    cfg.market_sources
        .sort_by_key(|s| if s.kind == "skillhub" { 0 } else { 1 });
    // 补齐每个 agent 的配置项
    for a in &cfg.agents {
        cfg.agent_configs.entry(a.id.clone()).or_default();
    }
    cfg
}

fn save_config(state: &AppState) -> Result<(), String> {
    let dir = state
        .handle
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let cfg = state.config.lock().unwrap();
    let json = serde_json::to_string_pretty(&*cfg).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("config.json"), json).map_err(|e| e.to_string())?;
    Ok(())
}

fn find_agent<'a>(cfg: &'a AppConfig, id: &str) -> Option<&'a AgentDef> {
    cfg.agents.iter().find(|a| a.id == id)
}

/// 常见 Agent 的友好显示名；未收录的做首字母大写兜底
fn pretty_agent_name(id: &str) -> String {
    let known = [
        ("trae", "Trae"),
        ("trae-cn", "Trae CN"),
        ("qwen", "Qwen Code"),
        ("qwenworkcn", "Qwen Work"),
        ("gemini", "Gemini CLI"),
        ("iflow", "iFlow CLI"),
        ("crush", "Crush"),
        ("opencode", "OpenCode"),
        ("copilot", "GitHub Copilot"),
        ("kimi-code", "Kimi Code"),
        ("kilocode", "Kilo Code"),
        ("roo-code", "Roo Code"),
        ("goose", "Goose"),
        ("droid", "Droid"),
    ];
    for (k, v) in known {
        if k == id {
            return v.to_string();
        }
    }
    let mut c = id.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => id.to_string(),
    }
}

// ================= 命令 =================

/// 全量同步数据（技能 + Agent 状态 + 配置）。
/// async：扫描要遍历整个技能库（重 I/O），放线程池执行，避免卡住窗口主线程。
#[tauri::command]
pub async fn get_sync_data(state: State<'_, AppState>) -> Result<SyncResult, String> {
    let cfg = state.config.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let skills = scan::scan_skills(&cfg.skills_root);
        let agents = cfg
            .agents
            .iter()
            .map(|a| deploy::agent_status(a, &cfg))
            .collect();
        SyncResult {
            skills,
            agents,
            config: cfg.clone(),
        }
    })
    .await
    .map_err(|e| format!("扫描任务异常: {e}"))
}

/// 重新扫描技能库
#[tauri::command]
pub async fn scan_skills_cmd(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, String> {
    let root = state.config.lock().unwrap().skills_root.clone();
    tauri::async_runtime::spawn_blocking(move || scan::scan_skills(&root))
        .await
        .map_err(|e| format!("扫描任务异常: {e}"))
}

/// 修改技能库根目录（可配置）
#[tauri::command]
pub fn set_skills_root(state: State<AppState>, root: String) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.skills_root = root;
    }
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

/// 保存 Agent 注册表（新增/编辑/删除）
#[tauri::command]
pub fn save_agents(state: State<AppState>, agents: Vec<AgentDef>) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.lock().unwrap();
        let ids: Vec<String> = agents.iter().map(|a| a.id.clone()).collect();
        cfg.agents = agents;
        for id in ids {
            cfg.agent_configs.entry(id).or_default();
        }
    }
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

/// 启用/禁用某 Agent 的某技能
#[tauri::command]
pub fn toggle_skill(
    state: State<AppState>,
    agent_id: String,
    skill_key: String,
    enabled: bool,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.lock().unwrap();
        if find_agent(&cfg, &agent_id).is_none() {
            return Err(format!("Agent 不存在: {agent_id}"));
        }
        let ac = cfg.agent_configs.entry(agent_id).or_default();
        if enabled {
            if !ac.enabled_skills.contains(&skill_key) {
                ac.enabled_skills.push(skill_key);
            }
        } else {
            ac.enabled_skills.retain(|k| k != &skill_key);
        }
    }
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

/// 计算某 Agent 的部署差异
#[tauri::command]
pub fn compute_diff_cmd(state: State<AppState>, agent_id: String) -> Vec<DiffItem> {
    let cfg = state.config.lock().unwrap();
    let Some(agent) = find_agent(&cfg, &agent_id) else {
        return Vec::new();
    };
    deploy::compute_diff(agent, &cfg)
}

/// 执行部署（dry_run=true 仅预览）
#[tauri::command]
pub fn deploy_cmd(
    state: State<AppState>,
    agent_id: String,
    dry_run: bool,
) -> Result<DeployResult, String> {
    let mut result = {
        let cfg = state.config.lock().unwrap();
        let Some(agent) = find_agent(&cfg, &agent_id) else {
            return Err(format!("Agent 不存在: {agent_id}"));
        };
        deploy::deploy(agent, &cfg, dry_run)
    };
    if !dry_run && !result.actions.is_empty() {
        let entry = AuditEntry {
            id: format!("{}-{}", now_ts(), agent_id),
            ts: now_ts(),
            action: "deploy".into(),
            agent_id: agent_id.clone(),
            detail: result.summary.clone(),
            actions: result.actions.clone(),
        };
        {
            let mut cfg = state.config.lock().unwrap();
            cfg.audit.push(entry);
            let overflow = cfg.audit.len().saturating_sub(200);
            if overflow > 0 {
                cfg.audit.drain(0..overflow);
            }
        }
        save_config(&state)?;
    }
    if !dry_run {
        // 刷新状态
        let cfg = state.config.lock().unwrap();
        if let Some(agent) = find_agent(&cfg, &agent_id) {
            let st = deploy::agent_status(agent, &cfg);
            result.summary = format!("{}｜{}", result.summary, st.message);
        }
    }
    Ok(result)
}

/// 回滚某 Agent 最近一次部署
#[tauri::command]
pub fn rollback_cmd(state: State<AppState>, agent_id: String) -> Result<DeployResult, String> {
    let result = {
        let cfg = state.config.lock().unwrap();
        deploy::rollback_last(&cfg, &agent_id)
    };
    if !result.actions.is_empty() && result.actions.iter().any(|a| a.ok) {
        let entry = AuditEntry {
            id: format!("{}-{}", now_ts(), agent_id),
            ts: now_ts(),
            action: "rollback".into(),
            agent_id: agent_id.clone(),
            detail: result.summary.clone(),
            actions: result.actions.clone(),
        };
        {
            let mut cfg = state.config.lock().unwrap();
            cfg.audit.push(entry);
            let overflow = cfg.audit.len().saturating_sub(200);
            if overflow > 0 {
                cfg.audit.drain(0..overflow);
            }
        }
        save_config(&state)?;
    }
    Ok(result)
}

/// 从技能库删除技能（移入 Windows 回收站，并清理关联的 Agent 链接与配置）
#[tauri::command]
pub fn delete_skill(state: State<AppState>, key: String) -> Result<DeleteResult, String> {
    let result = {
        let mut cfg = state.config.lock().unwrap();
        delete::delete_skill(&mut cfg, &key)?
    };
    // 破坏性操作留痕
    {
        let mut actions = result.removed_links.clone();
        actions.push(DeployAction {
            kind: "recycle_dir".into(),
            skill_key: result.skill_key.clone(),
            link_path: result.path.clone(),
            target: if result.recycled {
                "Windows 回收站".into()
            } else {
                String::new()
            },
            ok: true,
            message: result.message.clone(),
        });
        let mut cfg = state.config.lock().unwrap();
        cfg.audit.push(AuditEntry {
            id: format!("{}-del-{}", now_ts(), result.skill_key),
            ts: now_ts(),
            action: "delete_skill".into(),
            agent_id: "-".into(),
            detail: result.message.clone(),
            actions,
        });
        let overflow = cfg.audit.len().saturating_sub(200);
        if overflow > 0 {
            cfg.audit.drain(0..overflow);
        }
    }
    save_config(&state)?;
    Ok(result)
}

/// 清空审计日志
#[tauri::command]
pub fn clear_audit(state: State<AppState>) -> Result<(), String> {
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.audit.clear();
    }
    save_config(&state)
}

/// 刷新所有 Agent 状态
#[tauri::command]
pub fn refresh_status(state: State<AppState>) -> Vec<AgentStatus> {
    let cfg = state.config.lock().unwrap();
    cfg.agents
        .iter()
        .map(|a| deploy::agent_status(a, &cfg))
        .collect()
}

/// 扫描本机用户目录，发现已安装但未注册的 Agent skills 目录
/// （兼容目录被重定向到其他盘的情况：Junction 穿透后按最终路径判断）
#[tauri::command]
pub fn detect_agents(state: State<AppState>) -> Vec<DetectedAgent> {
    let cfg = state.config.lock().unwrap();
    let norm = |s: &str| s.replace('/', "\\").trim_end_matches('\\').to_lowercase();
    let registered: Vec<String> = cfg.agents.iter().map(|a| norm(&a.skills_dir)).collect();
    let mut out: Vec<DetectedAgent> = Vec::new();

    let home = match std::env::var("USERPROFILE") {
        Ok(h) => PathBuf::from(h),
        Err(_) => return out,
    };

    let mut push = |id: String, name: String, dir: PathBuf| {
        let key = norm(&dir.to_string_lossy());
        if registered.contains(&key) {
            return;
        }
        if out.iter().any(|d| norm(&d.skills_dir) == key) {
            return;
        }
        out.push(DetectedAgent {
            id,
            name,
            skills_dir: dir.to_string_lossy().to_string(),
        });
    };

    // 1. ~\.xxx\skills（点开头目录，含经过 Junction 重定向的）
    if let Ok(rd) = fs::read_dir(&home) {
        for e in rd.flatten() {
            let dn = e.file_name().to_string_lossy().to_string();
            if !dn.starts_with('.') || dn.len() < 2 {
                continue;
            }
            let skills = e.path().join("skills");
            if skills.is_dir() {
                let id = dn[1..].to_string();
                let name = pretty_agent_name(&id);
                push(id, name, skills);
            }
        }
    }

    // 2. 常见非点开头的安装位置
    let extras = [
        ("Windsurf", home.join(".codeium").join("windsurf").join("skills")),
        ("OpenCode", home.join(".config").join("opencode").join("skills")),
        ("Copilot", home.join(".copilot").join("skills")),
        ("Factory", home.join(".factory").join("skills")),
    ];
    for (name, dir) in extras {
        if dir.is_dir() {
            push(name.to_lowercase(), name.to_string(), dir);
        }
    }

    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// 把 Agent 当前实际链接（整目录链接内 + 独立链接）导入为启用配置
#[tauri::command]
pub fn import_existing_links(
    state: State<AppState>,
    agent_id: String,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.lock().unwrap();
        let Some(agent) = find_agent(&cfg, &agent_id) else {
            return Err(format!("Agent 不存在: {agent_id}"));
        };
        let keys = deploy::import_existing(agent, &cfg);
        let ac = cfg.agent_configs.entry(agent_id).or_default();
        for k in keys {
            if !ac.enabled_skills.contains(&k) {
                ac.enabled_skills.push(k);
            }
        }
    }
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

// ================= 技能市场 =================

/// 市场源列表
#[tauri::command]
pub fn market_sources(state: State<AppState>) -> Vec<MarketSource> {
    state.config.lock().unwrap().market_sources.clone()
}

/// 新增 GitHub 仓库市场源
#[tauri::command]
pub fn market_add_source(
    state: State<AppState>,
    name: String,
    repo: String,
    subpath: String,
) -> Result<AppConfig, String> {
    let repo = repo.trim().trim_matches('/').to_string();
    if !repo.contains('/') || repo.matches('/').count() != 1 {
        return Err("仓库格式应为 owner/repo".into());
    }
    {
        let mut cfg = state.config.lock().unwrap();
        let id = format!("github:{repo}");
        if cfg.market_sources.iter().any(|s| s.id == id) {
            return Err("该仓库源已存在".into());
        }
        cfg.market_sources.push(MarketSource {
            id,
            kind: "github".into(),
            name: if name.trim().is_empty() { repo.clone() } else { name.trim().to_string() },
            repo,
            subpath: subpath.trim().trim_matches('/').to_string(),
        });
    }
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

/// 删除市场源（内置 clawhub 源不可删）
#[tauri::command]
pub fn market_remove_source(state: State<AppState>, id: String) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.lock().unwrap();
        if id == "clawhub" {
            return Err("内置 ClawHub 源不可删除".into());
        }
        cfg.market_sources.retain(|s| s.id != id);
    }
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

/// 浏览市场（带错误信息的版本，供前端提示）。
///
/// 必须是 async 命令：同步命令跑在主线程上，市场要发多个 HTTP 请求
/// （超时上限 60s），期间整个窗口会被卡死、连加载动画都渲染不出来。
/// 这里只短暂持锁取参数，网络操作丢给 spawn_blocking 的线程池。
#[tauri::command]
pub async fn market_list_ex(
    state: State<'_, AppState>,
    source_id: String,
    query: String,
    page: u32,
) -> Result<Vec<MarketSkill>, String> {
    let task = {
        let cfg = state.config.lock().unwrap();
        let skills_root = cfg.skills_root.clone();
        let Some(src) = cfg.market_sources.iter().find(|s| s.id == source_id) else {
            return Err(format!("市场源不存在: {source_id}"));
        };
        let kind = src.kind.clone();
        let repo = src.repo.clone();
        let subpath = src.subpath.clone();
        move || -> Result<Vec<MarketSkill>, String> {
            match kind.as_str() {
                "clawhub" => {
                    // 空关键词 = 默认推荐（下载量/stars 精选合并，按页懒加载）；
                    // 有关键词才走搜索
                    if query.trim().is_empty() {
                        market::clawhub_browse(&skills_root, page)
                    } else {
                        market::clawhub_search(&query, &skills_root)
                    }
                }
                "skillhub" => market::skillhub_search(&query, &skills_root, page),
                "github" => market::github_list(&repo, &subpath, &skills_root),
                _ => Err(format!("未知源类型: {kind}")),
            }
        }
    };
    tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|e| format!("市场任务异常: {e}"))?
}

/// 从市场安装技能到技能库根目录（async：下载+解压较慢，避免卡住窗口）
#[tauri::command]
pub async fn market_install(
    state: State<'_, AppState>,
    source_id: String,
    reference: String,
    owner: String,
    icon_url: String,
    force: bool,
) -> Result<String, String> {
    let task = {
        let cfg = state.config.lock().unwrap();
        let skills_root = cfg.skills_root.clone();
        let Some(src) = cfg.market_sources.iter().find(|s| s.id == source_id) else {
            return Err(format!("市场源不存在: {source_id}"));
        };
        let kind = src.kind.clone();
        let repo = src.repo.clone();
        move || -> Result<String, String> {
            match kind.as_str() {
                "clawhub" => {
                    // reference 为 "owner/slug" 或纯 slug
                    let (o, slug) = match reference.split_once('/') {
                        Some((a, b)) => (a.to_string(), b.to_string()),
                        None => (String::new(), reference.clone()),
                    };
                    let o = if owner.is_empty() { o } else { owner };
                    // force = 更新：旧版本先移入应用回收站，失败可还原
                    if force {
                        delete::force_clear_for_update(&skills_root, &slug)?;
                    }
                    market::clawhub_install(&slug, &o, &skills_root, &icon_url)
                }
                "github" => {
                    if force {
                        let slug = reference.rsplit('/').next().unwrap_or("");
                        delete::force_clear_for_update(&skills_root, slug)?;
                    }
                    market::github_install(&repo, &reference, &skills_root)
                }
                "skillhub" => {
                    if force {
                        delete::force_clear_for_update(&skills_root, &reference)?;
                    }
                    market::skillhub_install(&reference, &owner, &skills_root, &icon_url)
                }
                _ => Err(format!("未知源类型: {kind}")),
            }
        }
    };
    let r = tauri::async_runtime::spawn_blocking(task)
        .await
        .map_err(|e| format!("安装任务异常: {e}"))?;
    if r.is_ok() {
        save_config(&state)?;
    }
    r
}

// ================= 技能详情 / 回收站 / 收藏标签 / Git 备份 =================

/// 读取技能详情（SKILL.md 全文 + 文件清单 + 风险提示）
#[tauri::command]
pub async fn read_skill_detail(
    state: State<'_, AppState>,
    key: String,
) -> Result<SkillDetail, String> {
    let root = state.config.lock().unwrap().skills_root.clone();
    tauri::async_runtime::spawn_blocking(move || scan::read_skill_detail(&root, &key))
        .await
        .map_err(|e| format!("详情任务异常: {e}"))?
}

/// 应用内回收站列表
#[tauri::command]
pub async fn list_trash(state: State<'_, AppState>) -> Result<Vec<TrashItem>, String> {
    let root = state.config.lock().unwrap().skills_root.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(delete::list_trash(&root)))
        .await
        .map_err(|e| format!("回收站任务异常: {e}"))?
}

/// 从应用内回收站还原技能
#[tauri::command]
pub async fn restore_trash(state: State<'_, AppState>, name: String) -> Result<String, String> {
    let root = state.config.lock().unwrap().skills_root.clone();
    let r = tauri::async_runtime::spawn_blocking(move || delete::restore_from_trash(&root, &name))
        .await
        .map_err(|e| format!("还原任务异常: {e}"))?;
    if r.is_ok() {
        save_config(&state)?;
    }
    r
}

/// 彻底删除回收站里的技能（移入 Windows 回收站）
#[tauri::command]
pub async fn purge_trash(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let root = state.config.lock().unwrap().skills_root.clone();
    tauri::async_runtime::spawn_blocking(move || delete::purge_trash(&root, &name))
        .await
        .map_err(|e| format!("清理任务异常: {e}"))?
}

/// 设置技能的用户标注（收藏 / 标签）
#[tauri::command]
pub fn set_skill_meta(
    state: State<AppState>,
    key: String,
    starred: bool,
    tags: Vec<String>,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.lock().unwrap();
        let meta = cfg.skill_meta.entry(key).or_default();
        meta.starred = starred;
        meta.tags = tags.into_iter().map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect();
    }
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

/// 技能库 Git 备份：add + commit（无 .git 时自动 init）
#[tauri::command]
pub async fn git_backup(state: State<'_, AppState>, message: String) -> Result<String, String> {
    let root = state.config.lock().unwrap().skills_root.clone();
    let msg = if message.trim().is_empty() {
        format!("AgentSkillHub 快照 {}", now_ts())
    } else {
        message.trim().to_string()
    };
    tauri::async_runtime::spawn_blocking(move || git_backup_sync(&root, &msg))
        .await
        .map_err(|e| format!("备份任务异常: {e}"))?
}

fn git_backup_sync(root: &str, message: &str) -> Result<String, String> {
    let run = |args: &[&str]| -> Result<String, String> {
        let out = hidden_command("git")
            .args(args)
            .current_dir(root)
            .output()
            .map_err(|e| format!("调用 git 失败: {e}"))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if !out.status.success() {
            return Err(text.lines().last().unwrap_or("git 命令失败").to_string());
        }
        Ok(text)
    };
    if !PathBuf::from(root).join(".git").is_dir() {
        run(&["init"])?;
    }
    run(&["add", "-A"])?;
    // 没有变更时 commit 会非零退出，单独识别这种情况
    match run(&["commit", "-m", message]) {
        Ok(t) => Ok(t.lines().next().unwrap_or("已提交").to_string()),
        Err(e) => {
            let status = run(&["status", "--porcelain"]).unwrap_or_default();
            if status.trim().is_empty() {
                Ok("没有变更，无需备份".into())
            } else {
                Err(e)
            }
        }
    }
}

// ================= 翻译 =================

/// 免费翻译接口（MyMemory，无需密钥），用于把英文技能描述翻成中文。
/// 结果按原文缓存到配置目录 translations.json，同一描述只请求一次。
#[tauri::command]
pub async fn translate_text(state: State<'_, AppState>, text: String) -> Result<String, String> {
    // 磁盘缓存先查
    let cache_key = text.trim().to_string();
    if let Some(hit) = translation_cache_get(&state, &cache_key) {
        return Ok(hit);
    }
    let task: Result<String, String> = tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let url = format!(
            "https://api.mymemory.translated.net/get?q={}&langpair=en|zh-CN&de=agentskillhub%40applocal.dev",
            crate::commands::market::urlencoding_lite_pub(&text)
        );
        let out = hidden_command("curl")
            .args(["-sL", "--max-time", "20", &url])
            .output()
            .map_err(|e| format!("调用翻译接口失败: {e}"))?;
        let v: serde_json::Value = serde_json::from_slice(&out.stdout)
            .map_err(|e| format!("解析翻译响应失败: {e}"))?;
        let translated = v
            .pointer("/responseData/translatedText")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        // MyMemory 出错时会把错误文本塞进 translatedText（如 MYMEMORY WARNING / QUERY LENGTH LIMIT）
        if translated.is_empty()
            || translated.to_uppercase().contains("MYMEMORY")
            || translated.to_uppercase().contains("WARNING")
        {
            return Err("翻译服务暂不可用".into());
        }
        Ok(translated)
    })
    .await
    .map_err(|e| format!("翻译任务异常: {e}"))
    .and_then(|inner| inner);
    if let Ok(t) = &task {
        let _ = translation_cache_put(&state, &cache_key, t);
    }
    task
}

fn translation_cache_path(state: &State<'_, AppState>) -> Option<std::path::PathBuf> {
    state
        .handle
        .path()
        .app_config_dir()
        .ok()
        .map(|d| d.join("translations.json"))
}

fn translation_cache_get(state: &State<'_, AppState>, key: &str) -> Option<String> {
    let path = translation_cache_path(state)?;
    let content = std::fs::read_to_string(path).ok()?;
    let map: std::collections::HashMap<String, String> = serde_json::from_str(&content).ok()?;
    map.get(key).cloned()
}

fn translation_cache_put(state: &State<'_, AppState>, key: &str, val: &str) -> Result<(), String> {
    let Some(path) = translation_cache_path(state) else {
        return Err("无法定位配置目录".into());
    };
    let mut map: std::collections::HashMap<String, String> = std::fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_default();
    map.insert(key.to_string(), val.to_string());
    let json = serde_json::to_string(&map).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// 给已安装技能补官方图标（从市场拿到的 icon_url），落盘为 _icon.*
#[tauri::command]
pub async fn ensure_skill_icon(
    state: State<'_, AppState>,
    slug: String,
    icon_url: String,
) -> Result<String, String> {
    let root = state.config.lock().unwrap().skills_root.clone();
    tauri::async_runtime::spawn_blocking(move || market::ensure_skill_icon(&slug, &icon_url, &root))
        .await
        .map_err(|e| format!("图标任务异常: {e}"))?
}

// ================= 外部技能（实体目录）=================

/// 列出某 Agent skills 目录中的实体技能（非链接）
#[tauri::command]
pub fn list_external_skills(state: State<AppState>, agent_id: String) -> Vec<ExternalSkill> {
    let cfg = state.config.lock().unwrap();
    let Some(agent) = find_agent(&cfg, &agent_id) else {
        return Vec::new();
    };
    market::list_external(&agent.skills_dir)
}

/// 收编外部技能：复制进技能库根目录
#[tauri::command]
pub fn adopt_skill(
    state: State<AppState>,
    agent_id: String,
    dir_name: String,
) -> Result<String, String> {
    let cfg = state.config.lock().unwrap();
    let Some(agent) = find_agent(&cfg, &agent_id) else {
        return Err(format!("Agent 不存在: {agent_id}"));
    };
    market::adopt(&agent.skills_dir, &dir_name, &cfg.skills_root)
}
