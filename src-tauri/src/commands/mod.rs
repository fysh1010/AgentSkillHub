pub mod cache;
pub mod agent_policy;
pub mod delete;
pub mod deploy;
pub mod discovery;
pub mod known_agents;
pub mod manifest;
pub mod market;
pub mod scan;
pub mod security;
pub mod tokens;
pub mod mcp;
pub mod mcp_catalog;
pub mod mcp_probe;
pub mod skill_sync;
pub mod updates;

use crate::models::*;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

pub struct AppState {
    pub config: Mutex<AppConfig>,
    pub handle: AppHandle,
    pub inventory: Mutex<Option<MachineInventory>>,
}

/// 用户主目录（Windows 下 %USERPROFILE%，其他平台回退 $HOME）
fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).filter(|p| p.is_absolute())
}

fn user_home() -> Option<PathBuf> {
    env_dir("USERPROFILE").or_else(|| env_dir("HOME"))
}

/// 已知清单的解析锚点
fn agent_bases() -> Option<known_agents::Bases> {
    Some(known_agents::Bases {
        home: user_home()?,
        local_appdata: env_dir("LOCALAPPDATA"),
        appdata: env_dir("APPDATA"),
    })
}

/// 内置 Agent 默认注册表（路径可在设置中修改）
/// 取 known_agents 已知清单里的常用产品，按当前用户目录推导；
/// 拿不到主目录时返回空表，由 detect_agents 兜底发现。
fn default_agents() -> Vec<AgentDef> {
    // 首次启动不虚构已安装工具；扫描只读，登记由用户开启配置时完成。
    Vec::new()
}

/// 老版本内置路径迁移到当前用户；有效路径和用户自定义路径原样保留。
fn migrate_legacy_agent_paths(agents: &mut [AgentDef]) {
    let Some(bases) = agent_bases() else { return };
    for a in agents {
        if let Some(path) = discovery::portable_agent_path(a, &bases) { a.skills_dir = path; }
    }
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
        cfg.skills_root = user_home().map(|p| p.join("AgentSkillHub/skills").to_string_lossy().to_string())
            .unwrap_or_default();
    }
    if cfg.agents.is_empty() {
        cfg.agents = default_agents();
    } else {
        migrate_legacy_agent_paths(&mut cfg.agents);
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

fn configurable_agent<'a>(cfg: &'a AppConfig, state: &AppState, id: &str) -> Result<&'a AgentDef, String> {
    agent_policy::require_agent(cfg, state.inventory.lock().unwrap().as_ref(), id)
}

/// 追加一条审计记录（200 条封顶）
fn push_audit(cfg: &mut AppConfig, entry: AuditEntry) {
    cfg.audit.push(entry);
    let overflow = cfg.audit.len().saturating_sub(200);
    if overflow > 0 {
        cfg.audit.drain(0..overflow);
    }
}

/// Agent 目录名的友好显示名：优先查内置已知清单，其余首字母大写兜底
fn pretty_agent_name(id: &str) -> String {
    if let Some(n) = known_agents::name_for(id) {
        return n.to_string();
    }
    let aliases = [
        ("qwenworkcn", "Qwen Work"),
        ("kilocode", "Kilo Code"),
        ("roo-code", "Roo Code"),
        ("droid", "Factory Droid"),
    ];
    for (k, v) in aliases {
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
    let active = agent_policy::active_config(&cfg, state.inventory.lock().unwrap().as_ref());
    tauri::async_runtime::spawn_blocking(move || {
        let skills = scan::scan_skills(&cfg.skills_root);
        let agents = active
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

/// 修改已允许配置的 Agent 的技能分配
#[tauri::command]
pub fn toggle_skill(
    state: State<AppState>,
    agent_id: String,
    skill_key: String,
    enabled: bool,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.lock().unwrap();
        configurable_agent(&cfg, &state, &agent_id)?;
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
pub fn compute_diff_cmd(state: State<AppState>, agent_id: String) -> Result<Vec<DiffItem>, String> {
    let cfg = state.config.lock().unwrap();
    let agent = configurable_agent(&cfg, &state, &agent_id)?;
    Ok(deploy::compute_diff(agent, &cfg))
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
        let agent = configurable_agent(&cfg, &state, &agent_id)?;
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
        configurable_agent(&cfg, &state, &agent_id)?;
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
        let allowed = agent_policy::active_config(&cfg,state.inventory.lock().unwrap().as_ref()).agents.into_iter().map(|a| a.id).collect();
        delete::delete_skill_with_access(&mut cfg, &key, &allowed)?
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
    agent_policy::active_config(&cfg, state.inventory.lock().unwrap().as_ref()).agents
        .iter()
        .map(|a| deploy::agent_status(a, &cfg))
        .collect()
}

/// 兼容旧界面的未接入目录查询，复用本机发现规则。
#[tauri::command]
pub async fn detect_agents(state: State<'_, AppState>) -> Result<Vec<DetectedAgent>, String> {
    let registered = state.config.lock().unwrap().agents.clone();
    let bases = agent_bases().ok_or("无法定位当前用户目录")?;
    tauri::async_runtime::spawn_blocking(move || discovery::inventory(&bases, &registered).agents
        .into_iter().filter(|a| !a.registered)
        .map(|a| DetectedAgent { id: a.id, name: a.name, skills_dir: a.skills_dir, group: String::new() }).collect())
        .await.map_err(|e| format!("发现 Agent 失败：{e}"))
}

/// 只读扫描，绝不自动登记或重新开启用户关闭的 Agent。
#[tauri::command]
pub async fn refresh_discovery(state: State<'_, AppState>) -> Result<MachineInventory, String> {
    let bases = agent_bases().ok_or("无法定位当前用户目录")?;
    let cfg = state.config.lock().unwrap().clone();
    let registered = cfg.agents.clone();
    let mut result = tauri::async_runtime::spawn_blocking(move || {
        let mut result = discovery::inventory(&bases, &registered);
        discovery::append_custom_sources(&mut result, &cfg.builtin_sources);
        result
    })
        .await.map_err(|e| format!("本机发现失败：{e}"))?;
    // 扫描期间开关可能被修改，使用提交时的最新配置进行标注。
    let current = state.config.lock().unwrap();
    agent_policy::annotate(&current, &mut result);
    *state.inventory.lock().unwrap() = Some(result.clone());
    Ok(result)
}

#[tauri::command]
pub fn set_agent_management(state: State<AppState>, agent_id: String, enabled: bool) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.lock().unwrap();
        let mut inventory = state.inventory.lock().unwrap();
        let inventory = inventory.as_mut().ok_or("请先完成本机扫描")?;
        agent_policy::set_enabled(&mut cfg, inventory, &agent_id, enabled)?;
    }
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

/// 自定义确认是明确的用户操作；有效程序文件或已存在技能目录至少提供一项。
#[tauri::command]
pub fn save_custom_agent(state: State<AppState>, agent: AgentDef, executable_path: String, enabled: bool) -> Result<AppConfig, String> {
    if agent.id.is_empty() || !agent.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("ID 只能包含字母、数字、连字符和下划线".into());
    }
    if agent.name.trim().is_empty() || !PathBuf::from(&agent.skills_dir).is_absolute() {
        return Err("请填写名称和绝对技能路径".into());
    }
    if !executable_path.is_empty() {
        if !PathBuf::from(&executable_path).is_absolute() || !PathBuf::from(&executable_path).is_file() {
            return Err("自定义程序路径必须是已存在的文件".into());
        }
    } else if !PathBuf::from(&agent.skills_dir).is_dir()
        && !state.inventory.lock().unwrap().as_ref().is_some_and(|inventory| inventory.agents.iter().any(|a| a.id==agent.id && a.installed && !a.shared)) {
        return Err("技能目录尚未存在，请提供本机已安装的程序文件路径".into());
    }
    {
        let mut cfg = state.config.lock().unwrap();
        let mut agent = agent;
        agent.builtin = known_agents::name_for(&agent.id).is_some();
        let id = agent.id.clone();
        if let Some(existing) = cfg.agents.iter_mut().find(|a| a.id == id) { *existing = agent; }
        else { cfg.agents.push(agent); }
        cfg.agent_preferences.insert(id.clone(), AgentPreference { enabled, executable_path, manual_confirmed:true });
        cfg.agent_configs.entry(id).or_default();
        // 修改路径后使旧扫描失效，后续写入必须等待新扫描，不能沿用旧路径证据。
        *state.inventory.lock().unwrap() = None;
    }
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

/// 把 Agent 当前实际链接（整目录链接内 + 独立链接）导入为启用配置
#[tauri::command]
pub fn import_existing_links(
    state: State<AppState>,
    agent_id: String,
) -> Result<AppConfig, String> {
    {
        let mut cfg = state.config.lock().unwrap();
        let agent = configurable_agent(&cfg, &state, &agent_id)?;
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

#[tauri::command]
pub async fn market_skill_detail(slug: String) -> Result<crate::models::MarketSkillDetail, String> {
    tauri::async_runtime::spawn_blocking(move || market::skillhub_detail(&slug))
        .await
        .map_err(|e| e.to_string())?
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
pub fn list_external_skills(state: State<AppState>, agent_id: String) -> Result<Vec<ExternalSkill>, String> {
    let cfg = state.config.lock().unwrap();
    let agent = configurable_agent(&cfg, &state, &agent_id)?;
    Ok(market::list_external(&agent.skills_dir))
}

/// 收编外部技能：复制进技能库根目录
#[tauri::command]
pub fn adopt_skill(
    state: State<AppState>,
    agent_id: String,
    dir_name: String,
) -> Result<String, String> {
    let cfg = state.config.lock().unwrap();
    let agent = configurable_agent(&cfg, &state, &agent_id)?;
    market::adopt(&agent.skills_dir, &dir_name, &cfg.skills_root)
}

// ================= 工具内置技能来源（插件缓存 / IDE 内置技能包）=================

/// 这些目录由对应工具自己写入与升级，本工具只读浏览、按需收编副本，绝不往里做链接部署。
/// 只从自动发现结果或用户已保存的来源中解析 ID。
fn builtin_source(source_id: &str, configs: &[CustomBuiltinSource]) -> Result<BuiltinSource, String> {
    if let Some(config) = configs.iter().find(|s| s.id == source_id) {
        return discovery::custom_source(config);
    }
    let bases = agent_bases().ok_or("无法定位用户主目录")?;
    discovery::source_by_id(&bases, source_id)
        .ok_or_else(|| format!("内置技能来源不存在或已被清理: {source_id}"))
}

/// 本机检测到的内置技能来源
#[tauri::command]
pub async fn detect_builtin_sources(state: State<'_, AppState>) -> Result<Vec<DetectedAgent>, String> {
    let configs = state.config.lock().unwrap().builtin_sources.clone();
    let Some(bases) = agent_bases() else {
        return Ok(Vec::new());
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut result = discovery::inventory(&bases, &[]);
        discovery::append_custom_sources(&mut result, &configs);
        result.sources.into_iter()
            .map(|s| DetectedAgent { id: s.id, name: s.name, skills_dir: s.skills_dir, group: s.group }).collect()
    })
        .await.map_err(|e| format!("内置技能发现失败：{e}"))
}

/// 某来源里的技能清单（只读）
#[tauri::command]
pub async fn builtin_source_skills(state: State<'_, AppState>, source_id: String) -> Result<Vec<ExternalSkill>, String> {
    let configs = state.config.lock().unwrap().builtin_sources.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let source = builtin_source(&source_id, &configs)?;
        Ok(discovery::selected_source_skills(&source))
    }).await.map_err(|e| format!("读取内置技能失败：{e}"))?
}

/// 收编某来源里的一个技能到技能库（原目录不动）
#[tauri::command]
pub async fn adopt_builtin_skill(
    state: State<'_, AppState>,
    source_id: String,
    dir_name: String,
) -> Result<String, String> {
    let root = state.config.lock().unwrap().skills_root.clone();
    let configs = state.config.lock().unwrap().builtin_sources.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let source = builtin_source(&source_id, &configs)?;
        if !source.skill_dirs.iter().any(|name| name == &dir_name) {
            return Err("来源中不存在该标准技能".into());
        }
        market::adopt(&source.skills_dir, &dir_name, &root)
    }).await.map_err(|e| format!("收编技能失败：{e}"))?
}

// ================= 安全扫描 =================

#[tauri::command]
pub fn save_builtin_source(state: State<AppState>, mut source: CustomBuiltinSource) -> Result<AppConfig, String> {
    source.tool_name = source.tool_name.trim().into();
    source.name = source.name.trim().into();
    source.skills_dir = source.skills_dir.trim().into();
    let checked = discovery::custom_source(&source)?;
    {
        let mut cfg = state.config.lock().unwrap();
        if !source.id.is_empty() && !cfg.builtin_sources.iter().any(|s| s.id == source.id) {
            return Err("来源已被移除，请重新添加".into());
        }
        let target = std::fs::canonicalize(&source.skills_dir).map_err(|e| format!("无法读取来源目录：{e}"))?;
        if cfg.builtin_sources.iter().any(|s| {
            let existing = std::fs::canonicalize(&s.skills_dir).unwrap_or_else(|_| PathBuf::from(&s.skills_dir));
            s.id != source.id && known_agents::eq_path(&existing, &target)
        }) {
            return Err("此技能目录已配置，请编辑已有来源".into());
        }
        source.id = checked.id;
        source.skills_dir = checked.skills_dir;
        if let Some(existing) = cfg.builtin_sources.iter_mut().find(|s| s.id == source.id) { *existing = source; }
        else { cfg.builtin_sources.push(source); }
    }
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

/// 仅移除本应用的来源记录，不删除或修改来源文件。
#[tauri::command]
pub fn remove_builtin_source(state: State<AppState>, id: String) -> Result<AppConfig, String> {
    state.config.lock().unwrap().builtin_sources.retain(|s| s.id != id);
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

/// 单技能内容级安全扫描（async：要读技能目录内多个文本文件）
#[tauri::command]
pub async fn scan_skill_security(
    state: State<'_, AppState>,
    key: String,
) -> Result<SecurityReport, String> {
    let root = state.config.lock().unwrap().skills_root.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(security::scan_skill_security(&root, &key)))
        .await
        .map_err(|e| format!("安全扫描任务异常: {e}"))?
}

/// 全库安全扫描汇总（技能列表 badge 用）
#[tauri::command]
pub async fn scan_security_summary(
    state: State<'_, AppState>,
) -> Result<Vec<SecuritySummaryItem>, String> {
    let root = state.config.lock().unwrap().skills_root.clone();
    tauri::async_runtime::spawn_blocking(move || Ok(security::scan_security_summary(&root)))
        .await
        .map_err(|e| format!("安全扫描任务异常: {e}"))?
}

// ================= Token 上下文预算 =================

/// 各 Agent 启用技能的 token 统计（async：要读每个技能的 SKILL.md，带缓存）
#[tauri::command]
pub async fn token_stats(state: State<'_, AppState>) -> Result<Vec<AgentTokenStats>, String> {
    let cfg = {
        let full = state.config.lock().unwrap();
        agent_policy::active_config(&full, state.inventory.lock().unwrap().as_ref())
    };
    tauri::async_runtime::spawn_blocking(move || {
        Ok(cfg
            .agents
            .iter()
            .map(|a| tokens::agent_token_stats(a, &cfg))
            .collect())
    })
    .await
    .map_err(|e| format!("token 统计任务异常: {e}"))?
}

/// 设置每个 Agent 的常驻上下文预算（token 数）
#[tauri::command]
pub fn set_token_budget(state: State<AppState>, budget: u64) -> Result<AppConfig, String> {
    if budget < 1000 {
        return Err("预算不能低于 1000 tokens".into());
    }
    {
        let mut cfg = state.config.lock().unwrap();
        cfg.token_budget = budget;
    }
    save_config(&state)?;
    Ok(state.config.lock().unwrap().clone())
}

// ================= 环境清单导出/导入 =================

/// 导出环境清单到技能库根目录，返回落盘路径
#[tauri::command]
pub fn export_manifest(state: State<AppState>) -> Result<String, String> {
    let cfg = state.config.lock().unwrap();
    manifest::write_manifest(&cfg, env!("CARGO_PKG_VERSION"))
}

/// 导入环境清单（覆盖分配关系），写入审计日志
#[tauri::command]
pub fn import_manifest(
    state: State<AppState>,
    path: String,
) -> Result<ManifestImportResult, String> {
    let r = {
        let mut cfg = state.config.lock().unwrap();
        let allowed = agent_policy::active_config(&cfg,state.inventory.lock().unwrap().as_ref()).agents.into_iter().map(|a| a.id).collect();
        let r = manifest::import_manifest(&mut cfg, &path, &allowed)?;
        migrate_legacy_agent_paths(&mut cfg.agents);
        push_audit(
            &mut cfg,
            AuditEntry {
                id: format!("{}-manifest", now_ts()),
                ts: now_ts(),
                action: "import_manifest".into(),
                agent_id: "-".into(),
                detail: r.message.clone(),
                actions: Vec::new(),
            },
        );
        r
    };
    save_config(&state)?;
    Ok(r)
}

/// 技能库根目录里最新的清单文件路径
#[tauri::command]
pub fn latest_manifest(state: State<AppState>) -> Option<String> {
    let cfg = state.config.lock().unwrap();
    manifest::latest_manifest_path(&cfg.skills_root)
}

/// 在资源管理器中显示某个文件（通用小命令）
#[tauri::command]
pub fn reveal_path(path: String) -> Result<(), String> {
    let target = PathBuf::from(&path);
    if !target.is_absolute() || !target.exists() {
        return Err(format!("路径不存在: {path}"));
    }
    let mut command = hidden_command("explorer.exe");
    if !target.is_dir() { command.arg("/select,"); }
    command.arg(path).spawn().map(|_| ()).map_err(|e| format!("打开资源管理器失败: {e}"))
}

/// 测试专用的干净临时目录：名字带 pid，避免并发/多进程跑测试时互相踩；
/// 返回前已清空同名旧目录（调用方自己 create_dir_all）。
#[cfg(test)]
pub(crate) fn test_scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ash_{}_{}", tag, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_agents_follow_current_user() {
        assert!(default_agents().is_empty(), "首次启动必须按检测结果登记，不能虚构已安装 Agent");
    }

    #[test]
    fn detected_agent_names_come_from_catalog() {
        assert_eq!(pretty_agent_name("trae-cn"), "Trae CN");
        assert_eq!(pretty_agent_name("kilo"), "Kilo Code");
        assert_eq!(pretty_agent_name("kilocode"), "Kilo Code");
        assert_eq!(pretty_agent_name("somefreshtool"), "Somefreshtool");
    }

    #[test]
    fn catalog_detection_on_this_machine_is_stable() {
        let Some(bases) = agent_bases() else { return };
        let once = known_agents::detect_existing(&bases);
        let twice = known_agents::detect_existing(&bases);
        assert_eq!(once.len(), twice.len());
        for h in &once {
            assert!(h.path.is_dir(), "命中目录应存在: {:?}", h.path);
        }
    }

    #[test]
    fn legacy_hardcoded_paths_get_rewritten() {
        let Some(home) = user_home() else { return };
        let home = home.to_string_lossy().to_string();
        let mut agents = vec![
            AgentDef {
                id: "claude".into(),
                name: "Claude Code".into(),
                skills_dir: r"C:\Users\previous-machine-user\.claude\skills".into(),
                builtin: true,
            },
            AgentDef {
                id: "mine".into(),
                name: "自建".into(),
                skills_dir: r"D:\custom\skills".into(),
                builtin: false,
            },
        ];
        migrate_legacy_agent_paths(&mut agents);
        assert_eq!(agents[0].skills_dir, format!(r"{home}\.claude\skills"));
        assert_eq!(agents[1].skills_dir, r"D:\custom\skills", "自建 Agent 不应被改动");
    }
}
