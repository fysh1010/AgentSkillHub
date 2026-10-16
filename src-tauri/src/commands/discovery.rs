//! 本机发现：产品约定、环境变量、PATH、Windows 安装登记与运行进程交叉验证。
//! 只枚举已知根目录；不扫全盘，不执行发现的程序，不修改任何 Agent 目录。

use super::{
    known_agents::{self, Bases, Root},
    scan,
};
use crate::models::*;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};

type Environment = HashMap<String, String>;
static SOURCE_CACHE: OnceLock<Mutex<Vec<BuiltinSource>>> = OnceLock::new();

#[derive(Debug, Clone, Default)]
struct Installation {
    id: String,
    dir: PathBuf,
    evidence: String,
    executable: PathBuf,
}

fn path_text(path: &Path) -> String {
    let text = path.to_string_lossy().into_owned();
    if cfg!(windows) {
        text.replace('/', "\\")
    } else {
        text
    }
}
fn resolved(path: &Path) -> String {
    path_text(&fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()))
        .trim_start_matches(r"\\?\")
        .to_string()
}
fn key(path: &Path) -> String {
    resolved(path)
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn logical_key(path: &Path) -> String {
    path_text(path)
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn override_home(id: &str, env: &Environment) -> Option<PathBuf> {
    let variable = match id {
        "codex" => "CODEX_HOME",
        "claude" => "CLAUDE_CONFIG_DIR",
        "gemini" => "GEMINI_CLI_HOME",
        "openclaw" => "OPENCLAW_STATE_DIR",
        _ => return None,
    };
    env.get(variable)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

pub fn skill_path(bases: &Bases, id: &str) -> Option<PathBuf> {
    skill_path_with_env(bases, id, &std::env::vars().collect())
}

fn skill_path_with_env(bases: &Bases, id: &str, env: &Environment) -> Option<PathBuf> {
    if let Some(root) = override_home(id, env) {
        return Some(root.join("skills"));
    }
    let agent = known_agents::KNOWN_AGENTS.iter().find(|a| a.id == id)?;
    if agent.rel.contains(&"*") {
        return None;
    }
    if id == "doubao" {
        let legacy = known_agents::template_path(bases, id).map(PathBuf::from);
        // 已有工作区优先，避免把旧配置从实际使用的位置迁走。
        if legacy.as_ref().is_some_and(|p| p.is_dir()) {
            return legacy;
        }
        return Some(bases.home.join("Doubao").join("skills"));
    }
    if agent.rel.first() == Some(&".config") {
        if let Some(root) = env
            .get("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
        {
            return Some(agent.rel[1..].iter().fold(root, |p, s| p.join(s)));
        }
    }
    known_agents::template_path(bases, id).map(PathBuf::from)
}

fn binary_names(id: &str) -> Vec<&str> {
    match id {
        "qoder-cli" => vec!["qodercli"],
        "aider-desk" => vec!["aider-desk", "aiderdesk"],
        "vibe" => vec!["vibe"],
        "hermes-agent" => vec!["hermes"],
        "autohand-code" => vec!["autohand"],
        "kiro" => vec!["kiro-cli"],
        "factory" => vec!["droid"],
        "kimi-code" => vec!["kimi"],
        "openclaw" => vec!["openclaw", "clawdbot", "moltbot"],
        "agents" | "config-agents" => vec![],
        _ if known_agents::name_for(id).is_some() => vec![id],
        _ => vec![],
    }
}

fn find_binary(id: &str, env: &Environment) -> Option<PathBuf> {
    let path = env
        .get("PATH")
        .or_else(|| env.get("Path"))
        .cloned()
        .unwrap_or_default();
    let mut dirs: Vec<_> = std::env::split_paths(&path).collect();
    // 桌面程序继承的 PATH 可能比用户环境旧；常用用户级 CLI 安装位置也参与只读检测。
    if let Some(home) = env.get("USERPROFILE").or_else(|| env.get("HOME")) {
        dirs.extend([".local/bin", ".cargo/bin", ".bun/bin"].map(|p| PathBuf::from(home).join(p)));
    }
    if let Some(appdata) = env.get("APPDATA") {
        dirs.push(PathBuf::from(appdata).join("npm"));
    }
    let mut seen = HashSet::new();
    for dir in dirs.into_iter().take(256) {
        if !dir.is_absolute() {
            continue;
        }
        if !seen.insert(logical_key(&dir)) {
            continue;
        }
        for name in binary_names(id) {
            for ext in if cfg!(windows) {
                &[".exe", ".cmd", ".bat", ".ps1"][..]
            } else {
                &[""][..]
            } {
                let file = dir.join(format!("{name}{ext}"));
                if file.is_file() {
                    return Some(file);
                }
            }
        }
    }
    None
}

// 产品名、程序名与目录名集中来自知识库；这里只补充不等于产品 ID 的发行名称。
fn product_aliases() -> Vec<(&'static str, &'static str)> {
    let mut aliases = vec![
        ("豆包", "doubao"),
        ("doubao", "doubao"),
        ("codex", "codex"),
        ("antigravity", "antigravity"),
        ("qoder cn", "qoder-cn"),
        ("trae cn", "trae-cn"),
        ("workbuddy", "workbuddy"),
        ("codebuddy", "codebuddy"),
        ("lingma", "lingma"),
        ("通义灵码", "lingma"),
        ("kiro", "kiro"),
        ("catpaw", "meituan-catpaw"),
        ("codearts", "codeartsdoer"),
    ];
    for agent in known_agents::KNOWN_AGENTS {
        if matches!(agent.id, "agents" | "config-agents") {
            continue;
        }
        aliases.push((agent.name, agent.id));
        // Claude Desktop 和 Windows Copilot 不等于 Claude Code / GitHub Copilot。
        if !matches!(agent.id, "claude" | "copilot") {
            aliases.push((agent.id, agent.id));
        }
    }
    aliases.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
    aliases.dedup();
    aliases
}

fn product_id(name: &str) -> Option<&'static str> {
    let n = name.trim().to_lowercase();
    product_aliases().into_iter().find_map(|(name, id)| {
        let prefix = name.to_lowercase();
        (n == prefix
            || n == format!("{prefix}.exe")
            || n.strip_prefix(&prefix).is_some_and(|tail| {
                let tail = tail.trim_start();
                tail.starts_with('(') || tail.chars().next().is_some_and(|c| c.is_ascii_digit())
            }))
        .then_some(id)
    })
}

fn product_executable(path: &Path, id: &str) -> bool {
    if !path.is_file()
        || !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
    {
        return false;
    }
    let name = path.file_stem().unwrap_or_default().to_string_lossy();
    product_id(&name) == Some(id)
        || binary_names(id)
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&name))
}

fn executable_names(id: &str) -> Vec<String> {
    let mut names: Vec<_> = product_aliases()
        .into_iter()
        .filter(|(_, target)| *target == id)
        .map(|(name, _)| format!("{name}.exe"))
        .collect();
    names.extend(binary_names(id).iter().map(|name| format!("{name}.exe")));
    names
}

fn common_installations(bases: &Bases, env: &Environment) -> Vec<Installation> {
    if !cfg!(windows) {
        return vec![];
    }
    let mut roots = vec![];
    if let Some(local) = &bases.local_appdata {
        roots.extend([local.join("Programs"), local.clone()]);
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = env
            .get(variable)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
        {
            roots.push(root);
        }
    }
    let mut out = Vec::new();
    for (folder, id) in product_aliases() {
        if out.iter().any(|a: &Installation| a.id == id) {
            continue;
        }
        for root in &roots {
            let dir = root.join(folder);
            if !dir.is_dir() {
                continue;
            }
            if let Some(executable) = executable_names(id)
                .iter()
                .map(|name| dir.join(name))
                .find(|p| product_executable(p, id))
            {
                out.push(Installation {
                    id: id.into(),
                    dir,
                    evidence: "常见安装位置".into(),
                    executable,
                });
                break;
            }
        }
    }
    out
}

/// 有限时的只读系统查询。仅输出产品名和路径，不读取进程命令行或用户凭据。
fn system_installations() -> (Vec<Installation>, Vec<String>) {
    #[cfg(not(windows))]
    {
        (Vec::new(), Vec::new())
    }
    #[cfg(windows)]
    {
        use std::{
            process::Stdio,
            time::{Duration, Instant},
        };
        let script = r#"
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$ErrorActionPreference = 'SilentlyContinue'
$productNames = @(__PRODUCT_NAMES__)
$pattern = '^(?:' + (($productNames | ForEach-Object { [regex]::Escape($_) }) -join '|') + ')(?:\s|\.|$)'
$roots = @('HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*','HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*')
$apps = @(Get-ItemProperty -Path $roots | Where-Object { $_.DisplayName -match $pattern } | ForEach-Object { @{name=$_.DisplayName;dir=$_.InstallLocation;icon=$_.DisplayIcon} })
$processes = @(Get-CimInstance Win32_Process | Where-Object { ($_.Name -replace '\.exe$','') -match $pattern } | ForEach-Object { @{name=$_.Name;icon=$_.ExecutablePath;dir=''} })
$menus = @([Environment]::GetFolderPath('StartMenu'),[Environment]::GetFolderPath('CommonStartMenu')) | Where-Object { $_ -and (Test-Path -LiteralPath $_) }
$shell = New-Object -ComObject WScript.Shell
$shortcuts = @(Get-ChildItem -LiteralPath $menus -Filter '*.lnk' -Recurse -File | Select-Object -First 1500 | Where-Object { $_.BaseName -match $pattern } | ForEach-Object { $link=$shell.CreateShortcut($_.FullName); @{name=$_.BaseName;icon=$link.TargetPath;dir=''} })
@{apps=$apps;processes=$processes;shortcuts=$shortcuts} | ConvertTo-Json -Depth 4 -Compress
"#;
        let names = product_aliases()
            .iter()
            .map(|(name, _)| format!("'{}'", name.replace('\'', "''")))
            .collect::<Vec<_>>()
            .join(",");
        let script = script.replace("__PRODUCT_NAMES__", &names);
        let mut child = match super::hidden_command("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => child,
            Err(e) => {
                return (
                    vec![],
                    vec![format!("系统安装信息不可用，已回退目录和 PATH 检测：{e}")],
                )
            }
        };
        // 边读边等，避免输出管道缓冲区填满后互相等待。
        let stdout = child.stdout.take().unwrap();
        let reader = std::thread::spawn(move || {
            use std::io::Read;
            let mut bytes = Vec::new();
            let _ = stdout.take(2 * 1024 * 1024).read_to_end(&mut bytes);
            bytes
        });
        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if start.elapsed() < Duration::from_secs(12) => {
                    std::thread::sleep(Duration::from_millis(40))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return (vec![], vec!["系统查询超时，目录与 PATH 检测仍可用".into()]);
                }
            }
        }
        let bytes = reader.join().unwrap_or_default();
        let data: serde_json::Value = match serde_json::from_slice(&bytes) {
            Ok(v) => v,
            Err(_) => {
                return (
                    vec![],
                    vec!["无法解析系统安装信息，已回退目录和 PATH 检测".into()],
                )
            }
        };
        let mut out = Vec::new();
        for (field, kind) in [
            ("apps", "安装登记"),
            ("processes", "运行进程"),
            ("shortcuts", "开始菜单"),
        ] {
            for row in data[field].as_array().into_iter().flatten() {
                let name = row["name"].as_str().unwrap_or_default();
                let Some(id) = product_id(name) else { continue };
                let dir = row["dir"].as_str().unwrap_or_default();
                let icon = row["icon"].as_str().unwrap_or_default().trim();
                let icon = icon
                    .rsplit_once(',')
                    .filter(|(_, s)| s.trim().parse::<i32>().is_ok())
                    .map(|(p, _)| p)
                    .unwrap_or(icon)
                    .trim_matches('"');
                let dir = if !dir.is_empty() && Path::new(dir).is_dir() {
                    PathBuf::from(dir)
                } else {
                    Path::new(icon)
                        .parent()
                        .unwrap_or(Path::new(""))
                        .to_path_buf()
                };
                // 图标或空安装目录可能是卸载残留，必须找到实际程序文件。
                let executable = if product_executable(Path::new(icon), id) {
                    Some(PathBuf::from(icon))
                } else {
                    executable_names(id)
                        .iter()
                        .map(|name| dir.join(name))
                        .find(|p| product_executable(p, id))
                };
                if let Some(executable) = executable.filter(|_| dir.is_absolute() && dir.is_dir()) {
                    out.push(Installation {
                        id: id.into(),
                        dir,
                        evidence: kind.into(),
                        executable,
                    });
                }
            }
        }
        (out, vec![])
    }
}

// 读取编辑器安装清单并核对扩展包，单独残留的目录或 .obsolete 项不能证明安装。
fn extension_installations(bases: &Bases) -> Vec<Installation> {
    let mut out = Vec::new();
    for editor in [
        ".vscode",
        ".vscode-insiders",
        ".cursor",
        ".windsurf",
        ".trae",
        ".trae-cn",
        ".qoder",
        ".qoder-cn",
    ] {
        let root = bases.home.join(editor).join("extensions");
        let read_json = |path: &Path| -> Option<serde_json::Value> {
            if fs::metadata(path).ok()?.len() > 2_000_000 {
                return None;
            }
            serde_json::from_slice(&fs::read(path).ok()?).ok()
        };
        let Some(list) = read_json(&root.join("extensions.json")) else {
            continue;
        };
        let obsolete = read_json(&root.join(".obsolete")).unwrap_or_default();
        for entry in list.as_array().into_iter().flatten().take(2048) {
            let id = entry["identifier"]["id"]
                .as_str()
                .unwrap_or_default()
                .to_lowercase();
            let agent_id = match id.as_str() {
                "saoudrizwan.claude-dev" => "cline",
                "rooveterinaryinc.roo-cline" => "roo",
                "kilocode.kilo-code" => "kilo",
                "continue.continue" => "continue",
                "augment.vscode-augment" => "augment",
                "github.copilot" | "github.copilot-chat" => "copilot",
                _ => continue,
            };
            let Some(relative) = entry["relativeLocation"].as_str() else {
                continue;
            };
            let rel_path = Path::new(relative);
            if rel_path.is_absolute()
                || rel_path.components().count() != 1
                || relative.starts_with('.')
                || obsolete[relative].as_bool() == Some(true)
            {
                continue;
            }
            let dir = root.join(relative);
            let manifest = dir.join("package.json");
            let Some(package) = read_json(&manifest) else {
                continue;
            };
            let declared = format!(
                "{}.{}",
                package["publisher"].as_str().unwrap_or_default(),
                package["name"].as_str().unwrap_or_default()
            );
            if declared.eq_ignore_ascii_case(&id) {
                out.push(Installation {
                    id: agent_id.into(),
                    dir,
                    evidence: "IDE 扩展".into(),
                    executable: manifest,
                });
            }
        }
    }
    out
}

fn evidence(kind: &str, path: &Path) -> DetectionEvidence {
    DetectionEvidence {
        kind: kind.into(),
        path: path_text(path),
    }
}

fn inspect_agent(
    id: String,
    name: String,
    path: PathBuf,
    known: bool,
    registered: bool,
    installs: &[Installation],
    env: &Environment,
) -> MachineAgent {
    let shared = id == "agents" || id == "config-agents";
    let mut proof = Vec::new();
    if path.is_dir() {
        proof.push(evidence("技能目录", &path));
    }
    if let Some(parent) = path.parent().filter(|p| p.is_dir()) {
        proof.push(evidence("配置目录", parent));
    }
    if let Some(p) = override_home(&id, env) {
        proof.push(evidence("环境变量", &p));
    }
    if let Some(binary) = find_binary(&id, env) {
        proof.push(evidence("PATH 命令", &binary));
    }
    let installation = installs.iter().find(|i| i.id == id);
    if let Some(i) = installation {
        proof.push(evidence(
            &i.evidence,
            if i.executable.as_os_str().is_empty() {
                &i.dir
            } else {
                &i.executable
            },
        ));
    }
    let confidence = if proof.iter().any(|e| {
        matches!(
            e.kind.as_str(),
            "安装登记"
                | "运行进程"
                | "PATH 命令"
                | "开始菜单"
                | "常见安装位置"
                | "IDE 扩展"
        )
    }) {
        "confirmed"
    } else if proof.is_empty() {
        "missing"
    } else if known || registered {
        "configured"
    } else {
        "possible"
    };
    let installed = confidence == "confirmed" && !shared;
    let executable_path = proof
        .iter()
        .find(|e| {
            matches!(
                e.kind.as_str(),
                "安装登记" | "运行进程" | "PATH 命令" | "开始菜单" | "常见安装位置"
            )
        })
        .map(|e| e.path.clone())
        .unwrap_or_default();
    MachineAgent {
        id,
        name,
        dir_exists: path.is_dir(),
        resolved_dir: resolved(&path),
        skills_dir: path_text(&path),
        registered,
        known,
        shared,
        confidence: confidence.into(),
        install_dir: installation.map(|i| path_text(&i.dir)).unwrap_or_default(),
        evidence: proof,
        installed,
        management_enabled: false,
        executable_path,
        custom: false,
    }
}

fn detect_with(
    bases: &Bases,
    registered: &[AgentDef],
    installs: &[Installation],
    env: &Environment,
) -> Vec<MachineAgent> {
    let mut out = Vec::new();
    let mut paths = HashSet::new();
    // 已保存的自定义路径具有优先级，不用推导路径覆盖用户设置。
    for a in registered {
        paths.insert(logical_key(Path::new(&a.skills_dir)));
        out.push(inspect_agent(
            a.id.clone(),
            a.name.clone(),
            PathBuf::from(&a.skills_dir),
            known_agents::name_for(&a.id).is_some(),
            true,
            installs,
            env,
        ));
    }
    for a in known_agents::KNOWN_AGENTS {
        let Some(path) = skill_path_with_env(bases, a.id, env) else {
            continue;
        };
        if out.iter().any(|item| item.id == a.id) {
            continue;
        }
        // 不同工具可以共用同一实际目录，不能按 Junction 目标合并产品。
        paths.insert(logical_key(&path));
        out.push(inspect_agent(
            a.id.into(),
            a.name.into(),
            path,
            true,
            false,
            installs,
            env,
        ));
    }
    // 按账号分目录的产品只接受确实存在的技能目录，不从通用父目录猜测账号。
    for hit in known_agents::detect_existing(bases) {
        if out.iter().any(|a| a.id == hit.agent_id()) {
            continue;
        }
        if paths.insert(logical_key(&hit.path)) {
            out.push(inspect_agent(
                hit.agent_id(),
                hit.display_name(),
                hit.path,
                true,
                false,
                installs,
                env,
            ));
        }
    }
    if let Ok(entries) = fs::read_dir(&bases.home) {
        for e in entries.flatten().take(512) {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.') || name.len() < 2 {
                continue;
            }
            let path = e.path().join("skills");
            if !path.is_dir() || !paths.insert(logical_key(&path)) {
                continue;
            }
            let id = name[1..].to_string();
            if out.iter().any(|a| a.id == id) {
                continue;
            }
            out.push(inspect_agent(
                id.clone(),
                super::pretty_agent_name(&id),
                path,
                false,
                false,
                installs,
                env,
            ));
        }
    }
    out.sort_by(|a, b| (!a.registered, a.shared, &a.name).cmp(&(!b.registered, b.shared, &b.name)));
    out
}

fn valid_skill_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<_> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .take(2048)
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("SKILL.md").is_file())
        .collect();
    dirs.sort();
    dirs
}

fn add_source(out: &mut Vec<BuiltinSource>, group: &str, name: &str, root: &Path) {
    let count = valid_skill_dirs(root).len();
    if count == 0
        || out
            .iter()
            .any(|s| key(Path::new(&s.skills_dir)) == key(root))
    {
        return;
    }
    // 路径摘要使中文、插件版本和标点实例不再撞 ID；规范路径去重 Junction 别名。
    let hash = key(root).bytes().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x100000001b3)
    });
    out.push(BuiltinSource {
        id: format!("source-{hash:016x}"),
        name: name.into(),
        group: group.into(),
        skills_dir: path_text(root),
        resolved_dir: resolved(root),
        skill_count: count,
    });
}

/// 仅在应用自身的插件包下有限下钻，不进入用户项目、连接器市场和网络缓存。
fn bundled_roots(
    out: &mut Vec<BuiltinSource>,
    group: &str,
    root: &Path,
    depth: usize,
    budget: &mut usize,
    visited: &mut HashSet<String>,
) {
    if *budget == 0 || depth == 0 || !root.is_dir() || !visited.insert(key(root)) {
        return;
    }
    *budget -= 1;
    if root
        .file_name()
        .is_some_and(|n| n == "skills" || n == ".skills")
    {
        let parent = root.parent().unwrap_or(root);
        let name = parent.file_name().unwrap_or_default().to_string_lossy();
        let name = if name.chars().next().is_some_and(|c| c.is_ascii_digit()) && name.contains('.')
        {
            format!(
                "{} / {}",
                parent
                    .parent()
                    .and_then(Path::file_name)
                    .unwrap_or_default()
                    .to_string_lossy(),
                name
            )
        } else {
            name.into_owned()
        };
        add_source(out, group, &name, root);
        return;
    }
    if let Ok(rd) = fs::read_dir(root) {
        for e in rd.flatten().take(256) {
            let name = e.file_name().to_string_lossy().into_owned();
            if matches!(
                name.as_str(),
                "node_modules" | ".git" | "cache" | "logs" | "vendor" | "projects" | "sessions"
            ) {
                continue;
            }
            bundled_roots(out, group, &e.path(), depth - 1, budget, visited);
        }
    }
}

fn collect_sources(
    bases: &Bases,
    installs: &[Installation],
    env: &Environment,
) -> Vec<BuiltinSource> {
    let mut out = Vec::new();
    for h in known_agents::detect_sources(bases) {
        // marketplace 是可用插件的目录，不能将其冒充已安装技能。
        if matches!(h.agent.id, "claude-plugins" | "claude-external-plugins") {
            continue;
        }
        add_source(
            &mut out,
            h.agent.name,
            h.instance.as_deref().unwrap_or(h.agent.name),
            &h.path,
        );
    }
    for (id, group) in [("codex", "Codex"), ("claude", "Claude Code")] {
        let root = override_home(id, env).unwrap_or_else(|| bases.home.join(format!(".{id}")));
        add_source(&mut out, group, "系统技能", &root.join("skills/.system"));
        let mut budget = 600;
        bundled_roots(
            &mut out,
            group,
            &root.join("plugins/cache"),
            5,
            &mut budget,
            &mut HashSet::new(),
        );
    }
    for (group, root) in [
        ("WorkBuddy", bases.home.join(".workbuddy/app")),
        ("WorkBuddy", bases.home.join(".workbuddy/plugins/cache")),
        ("豆包", bases.home.join("Doubao/.skills")),
    ] {
        bundled_roots(&mut out, group, &root, 6, &mut 600, &mut HashSet::new());
    }
    for installation in installs {
        let group = match installation.id.as_str() {
            "workbuddy" => "WorkBuddy",
            "doubao" => "豆包",
            _ => continue,
        };
        for relative in [
            "resources/app.asar.unpacked/resources/plugins",
            "resources/app/resources/plugins",
            "resources/skills",
        ] {
            bundled_roots(
                &mut out,
                group,
                &installation.dir.join(relative),
                7,
                &mut 1000,
                &mut HashSet::new(),
            );
        }
    }
    out.sort_by(|a, b| (&a.group, &a.name).cmp(&(&b.group, &b.name)));
    out
}

pub fn inventory(bases: &Bases, registered: &[AgentDef]) -> MachineInventory {
    let env = std::env::vars().collect();
    let (mut installs, warnings) = system_installations();
    installs.extend(extension_installations(bases));
    installs.extend(common_installations(bases, &env));
    let sources = collect_sources(bases, &installs, &env);
    *SOURCE_CACHE
        .get_or_init(|| Mutex::new(vec![]))
        .lock()
        .unwrap() = sources.clone();
    MachineInventory {
        home_dir: path_text(&bases.home),
        agents: detect_with(bases, registered, &installs, &env),
        sources,
        warnings,
        scanned_at: super::now_ts(),
        catalog_count: known_agents::KNOWN_AGENTS
            .iter()
            .filter(|a| !matches!(a.id, "agents" | "config-agents"))
            .count(),
    }
}

pub fn source_dir(bases: &Bases, id: &str) -> Option<PathBuf> {
    if let Some(source) = SOURCE_CACHE
        .get_or_init(|| Mutex::new(vec![]))
        .lock()
        .unwrap()
        .iter()
        .find(|s| s.id == id)
    {
        let dir = PathBuf::from(&source.skills_dir);
        return dir.is_dir().then_some(dir);
    }
    inventory(bases, &[])
        .sources
        .into_iter()
        .find(|s| s.id == id)
        .map(|s| PathBuf::from(s.skills_dir))
}

pub fn source_skills(dir: &Path) -> Vec<ExternalSkill> {
    valid_skill_dirs(dir)
        .into_iter()
        .map(|p| {
            let (name, display, description, _, healthy) =
                scan::read_skill_md_pub(&p.join("SKILL.md"));
            let dir_name = p
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            ExternalSkill {
                name: if !display.is_empty() {
                    display
                } else if !name.is_empty() {
                    name
                } else {
                    dir_name.clone()
                },
                dir_name,
                description,
                healthy,
                path: path_text(&p),
                system: true,
                size_bytes: 0,
            }
        })
        .collect()
}

/// 只迁移仍使用旧用户主目录约定的内置路径；自定义目录和有效目录始终保留。
pub fn portable_agent_path(agent: &AgentDef, bases: &Bases) -> Option<String> {
    if !agent.builtin {
        return None;
    }
    let template = known_agents::KNOWN_AGENTS
        .iter()
        .find(|a| a.id == agent.id)?;
    if let (Some(default), Some(current)) = (
        known_agents::template_path(bases, &agent.id),
        skill_path(bases, &agent.id),
    ) {
        if known_agents::eq_path(Path::new(&agent.skills_dir), Path::new(&default))
            && !known_agents::eq_path(Path::new(&default), &current)
        {
            return Some(path_text(&current));
        }
    }
    if Path::new(&agent.skills_dir).is_dir() {
        return None;
    }
    if template.rel.contains(&"*") {
        return None;
    }
    let original = agent.skills_dir.replace('\\', "/");
    let lower = original.to_lowercase();
    let marker = lower.find("/users/")?;
    let suffix = original[marker + 7..].find('/')? + marker + 7;
    let anchor = match template.root {
        Root::Home => "",
        Root::LocalAppData => "AppData/Local/",
        Root::AppData => "AppData/Roaming/",
    };
    let expected = format!("/{anchor}{}", template.rel.join("/"));
    if !original[suffix..].eq_ignore_ascii_case(&expected) {
        return None;
    }
    Some(path_text(&skill_path(bases, &agent.id)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bases(tag: &str) -> Bases {
        Bases {
            home: super::super::test_scratch(tag),
            local_appdata: None,
            appdata: None,
        }
    }
    fn make_skill(root: &Path, name: &str) {
        fs::create_dir_all(root.join(name)).unwrap();
        fs::write(
            root.join(name).join("SKILL.md"),
            format!("---\nname: {name}\ndescription: A real skill\n---\n"),
        )
        .unwrap();
    }
    #[test]
    fn catalog_names_expand_system_detection_without_accepting_uninstallers() {
        for (name, id) in [
            ("Google Antigravity", "antigravity"),
            ("Antigravity.exe", "antigravity"),
            ("Kiro", "kiro"),
            ("通义灵码", "lingma"),
            ("MiniMax Code 1.2.3", "minimax"),
        ] {
            assert_eq!(product_id(name), Some(id));
        }
        assert_eq!(product_id("Claude"), None);
        assert_eq!(product_id("Copilot"), None);
        assert_eq!(product_id("豆包输入法"), None);
        let root = super::super::test_scratch("known_executable");
        fs::create_dir_all(&root).unwrap();
        let uninstall = root.join("uninstall.exe");
        fs::write(&uninstall, b"fixture").unwrap();
        assert!(!product_executable(&uninstall, "antigravity"));
        let executable = root.join("Antigravity.exe");
        fs::write(&executable, b"fixture").unwrap();
        assert!(product_executable(&executable, "antigravity"));
        assert!(!product_executable(&executable, "codex"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn editor_install_registry_and_package_must_agree() {
        let root = super::super::test_scratch("extensions_installed");
        let bases = Bases {
            home: root.clone(),
            local_appdata: None,
            appdata: None,
        };
        let extensions = root.join(".vscode/extensions");
        let package = extensions.join("saoudrizwan.claude-dev-1.0.0");
        fs::create_dir_all(&package).unwrap();
        fs::write(
            package.join("package.json"),
            r#"{"publisher":"saoudrizwan","name":"claude-dev"}"#,
        )
        .unwrap();
        assert!(
            extension_installations(&bases).is_empty(),
            "仅扩展目录不能确认安装"
        );
        fs::write(extensions.join("extensions.json"),r#"[{"identifier":{"id":"saoudrizwan.claude-dev"},"relativeLocation":"saoudrizwan.claude-dev-1.0.0"}]"#).unwrap();
        let installed = extension_installations(&bases);
        assert_eq!(installed.len(), 1);
        let agents = detect_with(&bases, &[], &installed, &Environment::new());
        assert!(agents.iter().any(|a| a.id == "cline" && a.installed));
        assert!(!agents.iter().any(|a| a.id == "roo" && a.installed));
        fs::write(
            extensions.join(".obsolete"),
            r#"{"saoudrizwan.claude-dev-1.0.0":true}"#,
        )
        .unwrap();
        assert!(extension_installations(&bases).is_empty());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn detects_product_before_skills_directory_exists() {
        let b = bases("discovery_empty_skills");
        fs::create_dir_all(b.home.join(".cursor")).unwrap();
        let items = detect_with(&b, &[], &[], &Environment::new());
        let cursor = items.iter().find(|a| a.id == "cursor").unwrap();
        assert!(!cursor.dir_exists);
        assert_eq!(cursor.confidence, "configured");
        assert!(!items.iter().find(|a| a.id == "claude").unwrap().installed);
    }
    #[test]
    fn environment_and_custom_paths_take_precedence() {
        let b = bases("discovery_overrides");
        let custom = b.home.join("relocated-codex");
        let env = Environment::from([("CODEX_HOME".into(), path_text(&custom))]);
        let items = detect_with(&b, &[], &[], &env);
        assert_eq!(
            items.iter().find(|a| a.id == "codex").unwrap().skills_dir,
            path_text(&custom.join("skills"))
        );
        let configured = AgentDef {
            id: "codex".into(),
            name: "My Codex".into(),
            skills_dir: path_text(&b.home.join("mine")),
            builtin: false,
        };
        let items = detect_with(&b, &[configured.clone()], &[], &env);
        assert_eq!(items.iter().filter(|a| a.id == "codex").count(), 1);
        assert_eq!(items[0].skills_dir, configured.skills_dir);
    }

    #[test]
    #[cfg(windows)]
    fn shared_junction_target_does_not_merge_different_products() {
        let b = bases("discovery_shared_target");
        let shared = b.home.join("shared-skills");
        fs::create_dir_all(&shared).unwrap();
        for product in [".claude", ".codex"] {
            fs::create_dir_all(b.home.join(product)).unwrap();
            super::super::deploy::create_junction(&b.home.join(product).join("skills"), &shared)
                .unwrap();
        }
        let items = detect_with(&b, &[], &[], &Environment::new());
        assert!(items.iter().any(|a| a.id == "claude"));
        assert!(items.iter().any(|a| a.id == "codex"));
    }
    #[test]
    fn excludes_ime_updaters_and_distinguishes_china_editions() {
        assert_eq!(product_id("豆包输入法"), None);
        assert_eq!(product_id("DoubaoIME"), None);
        assert_eq!(
            product_id("Claude"),
            None,
            "Claude Desktop 不能被冒充为 Claude Code"
        );
        assert_eq!(product_id("WorkBuddy Updater"), None);
        assert_eq!(product_id("Qoder CN 0.4.8"), Some("qoder-cn"));
        assert_eq!(product_id("TRAE CN (User)"), Some("trae-cn"));
    }
    #[test]
    fn finds_workbuddy_bundles_and_flattens_system_skills() {
        let b = bases("discovery_bundles");
        let install = b.home.join("custom-install");
        make_skill(
            &install.join("resources/app.asar.unpacked/resources/plugins/workbuddy-builtin/skills"),
            "native-one",
        );
        make_skill(&install.join("resources/app.asar.unpacked/resources/plugins/workbuddy-builtin/builtin-plugins/docs/skills"), "native-docs");
        make_skill(&b.home.join(".workbuddy/skills/.system"), "system-one");
        fs::create_dir_all(b.home.join(".workbuddy/skills/.system/not-a-skill")).unwrap();
        let sources = collect_sources(
            &b,
            &[Installation {
                id: "workbuddy".into(),
                dir: install,
                evidence: "安装登记".into(),
                executable: PathBuf::new(),
            }],
            &Environment::new(),
        );
        assert_eq!(sources.iter().map(|s| s.skill_count).sum::<usize>(), 3);
        assert_eq!(sources.len(), 3);
        for source in sources {
            assert_eq!(source_skills(Path::new(&source.skills_dir)).len(), 1);
        }
    }
    #[test]
    fn portable_paths_do_not_depend_on_author_username() {
        let b = bases("discovery_portable");
        let mut agent = AgentDef {
            id: "claude".into(),
            name: "Claude".into(),
            skills_dir: "C:/Users/some-other-user/.claude/skills".into(),
            builtin: true,
        };
        assert_eq!(
            portable_agent_path(&agent, &b),
            Some(path_text(&b.home.join(".claude").join("skills")))
        );
        agent.skills_dir = "D:/MyCustomSkills".into();
        assert!(portable_agent_path(&agent, &b).is_none());
    }
    #[test]
    fn repairs_obsolete_doubao_default_without_overwriting_custom_path() {
        let mut b = bases("discovery_doubao_migration");
        b.local_appdata = Some(b.home.join("Local"));
        fs::create_dir_all(b.home.join("Doubao/skills")).unwrap();
        let mut agent = AgentDef {
            id: "doubao".into(),
            name: "豆包".into(),
            skills_dir: known_agents::template_path(&b, "doubao").unwrap(),
            builtin: true,
        };
        assert_eq!(
            portable_agent_path(&agent, &b),
            Some(path_text(&b.home.join("Doubao/skills")))
        );
        agent.skills_dir = path_text(&b.home.join("custom-doubao-skills"));
        assert!(portable_agent_path(&agent, &b).is_none());
        agent.skills_dir = r"C:\Users\old-account\AppData\Local\Doubao\User Data\Default\.doubao\agent_mode\workspace\.user_skills".into();
        assert_eq!(
            portable_agent_path(&agent, &b),
            Some(path_text(&b.home.join("Doubao/skills")))
        );
    }
    #[test]
    fn real_machine_inventory_is_read_only_and_sources_are_valid() {
        let Some(b) = super::super::agent_bases() else {
            return;
        };
        let inv = inventory(&b, &[]);
        assert!(!inv.home_dir.is_empty());
        for source in &inv.sources {
            assert!(source.skill_count > 0);
            assert_eq!(
                source_skills(Path::new(&source.skills_dir)).len(),
                source.skill_count
            );
        }
        println!("{}", serde_json::to_string(&inv).unwrap());
    }
}
