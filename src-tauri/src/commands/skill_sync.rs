//! 用户技能库专用 Git 同步；不访问本项目仓库，不保存凭证，不强推或自动解冲突。
use super::{hidden_command, scan, AppState};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::{ipc::Channel, Manager, State};

pub(super) static SYNC: Mutex<()> = Mutex::new(());

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SyncSettings {
    pub repository: String,
    pub branch: String,
    #[serde(default)]
    pub whole_repository: bool,
}

#[derive(Clone, Serialize)]
pub struct SyncProgress {
    step: u8,
    total: u8,
    label: &'static str,
}

fn validate(settings: &SyncSettings) -> Result<String, String> {
    let repository = settings.repository.trim().trim_end_matches('/');
    if repository.contains(['\\', '?', '#']) || repository.chars().any(char::is_whitespace) {
        return Err("仓库地址不能包含空白、查询参数或片段".into());
    }
    validate_branch(&settings.branch)?;
    if github_slug(repository).is_none() {
        if let Ok(url) = url::Url::parse(repository) {
            if matches!(url.scheme(), "https" | "ssh")
                && url.host_str().is_some()
                && !url.path().trim_matches('/').is_empty()
                && url.password().is_none()
                && (url.scheme() == "ssh" || url.username().is_empty())
            {
                return Ok(url.into());
            }
        }
        if let Some((user_host, path)) = repository.split_once(':') {
            if let Some((user, host)) = user_host.split_once('@') {
                let simple = |value: &str| {
                    !value.is_empty()
                        && !value.starts_with('-')
                        && value
                            .bytes()
                            .all(|ch| ch.is_ascii_alphanumeric() || b"-_.".contains(&ch))
                };
                if simple(user)
                    && simple(host)
                    && !path.is_empty()
                    && !path.starts_with('-')
                    && path
                        .bytes()
                        .all(|ch| ch.is_ascii_alphanumeric() || b"-_.~/".contains(&ch))
                {
                    return Ok(repository.into());
                }
            }
        }
    }
    let ssh = repository.starts_with("git@github.com:")
        || repository.starts_with("ssh://git@github.com/");
    let repository = github_slug(repository)
        .unwrap_or_else(|| repository.strip_suffix(".git").unwrap_or(repository));
    let parts: Vec<_> = repository.split('/').collect();
    if parts.len() != 2
        || parts.iter().any(|p| {
            p.is_empty()
                || *p == "."
                || *p == ".."
                || !p
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        })
    {
        return Err(
            "请输入 HTTPS 或 SSH Git 仓库地址，也支持 GitHub owner/repo。地址中不能包含令牌。"
                .into(),
        );
    }
    Ok(if ssh {
        format!("git@github.com:{repository}.git")
    } else {
        format!("https://github.com/{repository}.git")
    })
}

fn validate_branch(branch: &str) -> Result<(), String> {
    if branch.is_empty()
        || branch.starts_with('-')
        || branch.starts_with('.')
        || branch.ends_with('.')
        || branch.ends_with('/')
        || branch.contains("..")
        || branch.contains("//")
        || branch.contains("@{")
        || branch
            .split('/')
            .any(|s| s.is_empty() || s.starts_with('.') || s.ends_with(".lock"))
        || !branch
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./".contains(c))
    {
        return Err("分支名无效，例如 main 或 skills/shared".into());
    }
    Ok(())
}

fn github_slug(url: &str) -> Option<&str> {
    [
        "https://github.com/",
        "git@github.com:",
        "ssh://git@github.com/",
    ]
    .iter()
    .find_map(|prefix| url.strip_prefix(prefix))
    .map(|slug| slug.strip_suffix(".git").unwrap_or(slug))
}

fn same_remote(expected: &str, actual: &str) -> bool {
    match (github_slug(expected), github_slug(actual)) {
        (Some(left), Some(right)) => left.eq_ignore_ascii_case(right),
        _ => expected.trim_end_matches('/') == actual.trim_end_matches('/'),
    }
}

#[tauri::command]
pub async fn detect_skill_git(
    state: State<'_, AppState>,
    repository: Option<String>,
) -> Result<SyncSettings, String> {
    let root = PathBuf::from(state.config.lock().unwrap().skills_root.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let mut settings = SyncSettings {
            repository: repository.unwrap_or_default(),
            branch: "main".into(),
            whole_repository: false,
        };
        if root.join(".git").exists() {
            let raw = git(&root, &["remote", "get-url", "--all", "origin"]).unwrap_or_default();
            if raw.lines().count() == 1
                && (settings.repository.is_empty()
                    || same_remote(&validate(&settings)?, raw.trim()))
            {
                let branch = git(&root, &["symbolic-ref", "--short", "HEAD"])?;
                settings = SyncSettings {
                    repository: raw.trim().into(),
                    branch: branch.trim().into(),
                    whole_repository: true,
                };
                validate(&settings)?;
                return Ok(settings);
            }
        }
        if settings.repository.is_empty() {
            return Err("当前技能库没有可读取的 Git 远端，请填写仓库地址。".into());
        }
        let remote = validate(&settings)?;
        let output = run_git(
            &std::env::temp_dir(),
            &["ls-remote", "--symref", &remote, "HEAD"],
            true,
            false,
        )?
        .unwrap_or_default();
        if let Some(branch) = output.lines().find_map(|line| {
            line.strip_prefix("ref: refs/heads/")?
                .strip_suffix("\tHEAD")
        }) {
            validate_branch(branch)?;
            settings.branch = branch.into();
        }
        Ok(settings)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn settings_path(state: &AppState) -> Result<PathBuf, String> {
    let dir = state
        .handle
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("skill-sync.json"))
}

#[tauri::command]
pub fn skill_sync_settings(state: State<'_, AppState>) -> Result<SyncSettings, String> {
    let path = settings_path(&state)?;
    if !path.exists() {
        return Ok(SyncSettings {
            repository: String::new(),
            branch: "main".into(),
            whole_repository: false,
        });
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_skill_sync_settings(
    state: State<'_, AppState>,
    settings: SyncSettings,
) -> Result<(), String> {
    validate(&settings)?;
    super::mcp::write_recoverable(
        &settings_path(&state)?,
        &serde_json::to_vec_pretty(&settings).map_err(|e| e.to_string())?,
    )
}

fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    Ok(run_git(root, args, false, false)?.unwrap_or_default())
}

fn run_git(
    root: &Path,
    args: &[&str],
    login: bool,
    allow_no_match: bool,
) -> Result<Option<String>, String> {
    let mut command = hidden_command("git");
    command.args(["-c", "core.hooksPath=", "-c", "core.quotepath=false"]);
    #[cfg(not(test))]
    command.env("GIT_ALLOW_PROTOCOL", "https:ssh");
    if login {
        command
            .args(["-c", "credential.interactive=true"])
            .env_remove("GCM_INTERACTIVE");
    } else {
        command
            .args(["-c", "credential.interactive=false"])
            .env("GCM_INTERACTIVE", "Never");
    }
    let mut child = command
        .args(args)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("无法启动 Git，请先安装 Git：{e}"))?;
    #[cfg(windows)]
    let _job = super::mcp_probe::ProcessJob::try_attach(&child);
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let read = |stream: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stream
                .take(16 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        })
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            if out.is_finished() && err.is_finished() {
                break status;
            }
        }
        // Git 退出后，其子进程仍可能持有输出管道，读取也必须受同一个时限约束。
        if start.elapsed() >= Duration::from_secs(90) {
            let _ = child.kill();
            return Err("这条 Git 命令等待超过 90 秒；请检查网络和本机 Git 登录状态后重试".into());
        }
        std::thread::sleep(Duration::from_millis(40));
    };
    let stdout = out
        .join()
        .map_err(|_| "Git 输出读取失败")?
        .map_err(|e| e.to_string())?;
    let stderr = err
        .join()
        .map_err(|_| "Git 错误读取失败")?
        .map_err(|e| e.to_string())?;
    if stdout.len() > 16 * 1024 * 1024 {
        return Err("Git 返回的文件列表过大，已停止同步".into());
    }
    if allow_no_match && status.code() == Some(1) && stdout.is_empty() && stderr.is_empty() {
        return Ok(None);
    }
    if !status.success() {
        let message = String::from_utf8_lossy(&stderr);
        return Err(format!("Git 操作失败：{}。认证失败请先在本机 Git 中登录；分支分叉需手动合并，本工具不会强制覆盖。", message.chars().take(1800).collect::<String>()));
    }
    String::from_utf8(stdout)
        .map(Some)
        .map_err(|_| "Git 返回了非 UTF-8 路径".into())
}

pub(super) fn blocked_part(part: &str) -> bool {
    let name = part.to_ascii_lowercase();
    name.starts_with('.')
        || matches!(
            name.as_str(),
            "_archive"
                | "_tmp"
                | "node_modules"
                | "target"
                | "dist"
                | "cache"
                | "logs"
                | "credentials.json"
                | "auth.json"
                | "secrets.json"
                | "secrets.yaml"
                | "secrets.yml"
                | "mcp.json"
                | "mcp-config.json"
                | "id_rsa"
                | "id_ed25519"
                | "id_ecdsa"
                | "id_dsa"
        )
        || [
            ".pem",
            ".key",
            ".log",
            ".p12",
            ".pfx",
            ".ppk",
            ".kdbx",
            ".jks",
            ".keystore",
            ".env",
        ]
        .iter()
        .any(|ext| name.ends_with(ext))
}

fn repo_metadata(path: &str) -> bool {
    matches!(path, "README.md" | "LICENSE" | ".gitignore")
}

fn roots_for_paths(paths: &[String]) -> BTreeSet<String> {
    paths
        .iter()
        .filter_map(|p| p.strip_suffix("/SKILL.md").map(str::to_owned))
        .collect()
}

fn validate_paths(paths: &[String], roots: &BTreeSet<String>) -> Result<(), String> {
    for path in paths {
        if repo_metadata(path) {
            continue;
        }
        if path.contains(['\\', ':', '\r', '\n'])
            || path.split('/').any(blocked_part)
            || !roots
                .iter()
                .any(|root| path.starts_with(&format!("{root}/")))
        {
            return Err(format!(
                "仅同步技能模式已拦截此路径：{path}。若这是你自己的私有仓库，可启用「按 Git 同步整个仓库」。"
            ));
        }
    }
    Ok(())
}

// 检查实际暂存的文本；只报告路径，避免把命中的凭证值写进日志或界面。
fn check_staged_credentials(root: &Path) -> Result<(), String> {
    let output = run_git(
        root,
        &[
            "grep",
            "--cached",
            "-I",
            "-l",
            "-z",
            "-E",
            "-e",
            "-----BEGIN ([A-Z0-9]+ )*PRIVATE KEY-----",
            "-e",
            "(gh[pousr]_[A-Za-z0-9]{36}|github_pat_[A-Za-z0-9_]{60,}|AKIA[A-Z0-9]{16})",
        ],
        false,
        true,
    )?;
    if let Some(paths) = output {
        let first = paths.split('\0').next().unwrap_or_default();
        return Err(format!("暂存文件疑似含私钥、GitHub 令牌或 AWS 访问密钥 ID，已停止推送：{first}。请检查并将真实凭证改为本机读取"));
    }
    Ok(())
}

pub(super) fn local_files(path: &Path, root: &Path, paths: &mut Vec<String>) -> Result<(), String> {
    if path != root {
        let parent = path.parent().ok_or("同步路径无父目录")?;
        let mut wrapper = root.to_path_buf();
        for part in parent.strip_prefix(root).map_err(|_| "同步路径不在技能库内")?.components() {
            wrapper.push(part);
            if super::deploy::is_junction(&wrapper) {
                return Err(format!("同步不跟随分类目录链接：{}", wrapper.display()));
            }
        }
    }
    let metadata = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(format!(
                "同步不跟随目录联接：{}，请先收编副本",
                path.display()
            ));
        }
    }
    if metadata.file_type().is_symlink() {
        return Err(format!("同步不跟随符号链接：{}", path.display()));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            if blocked_part(&entry.file_name().to_string_lossy()) {
                continue;
            }
            local_files(&entry.path(), root, paths)?;
        }
    } else if metadata.is_file() {
        paths.push(
            path.strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/"),
        );
    }
    Ok(())
}

fn tree_paths(root: &Path, reference: &str) -> Result<Vec<String>, String> {
    let tree = git(root, &["ls-tree", "-rz", reference])?;
    tree.split('\0')
        .filter(|s| !s.is_empty())
        .map(|entry| {
            let (meta, path) = entry.split_once('\t').ok_or("无法解析远程目录")?;
            if !meta.starts_with("100644 ") && !meta.starts_with("100755 ") {
                return Err(format!("不允许同步链接或子模块：{path}"));
            }
            Ok(path.to_owned())
        })
        .collect()
}

fn sync(
    root: &Path,
    settings: &SyncSettings,
    operation: &str,
    mut report: impl FnMut(SyncProgress),
) -> Result<String, String> {
    let remote = validate(settings)?;
    if operation == "connect" {
        report(SyncProgress {
            step: 1,
            total: 1,
            label: "连接远端仓库（如弹出登录窗口，请完成登录）",
        });
        if root.is_dir() && root.join(".git").exists() {
            if let Ok(url) = git(root, &["remote", "get-url", "--all", "origin"]) {
                if url.lines().count() == 1 && same_remote(&remote, url.trim()) {
                    run_git(root, &["ls-remote", "--heads", "origin"], true, false)?;
                    return Ok(
                        "已连接 Git 仓库，当前 Git 账号可读取仓库。推送还需要写入权限。".into(),
                    );
                }
            }
        }
        // 首次登录不需要初始化技能库。
        let dir = std::env::temp_dir();
        run_git(&dir, &["ls-remote", "--heads", &remote], true, false)?;
        return Ok("已连接 Git 仓库，当前 Git 账号可读取仓库。推送还需要写入权限。".into());
    }
    sync_repository(
        root,
        &remote,
        &settings.branch,
        operation,
        settings.whole_repository,
        report,
    )
}

fn fast_forward(root: &Path, reference: &str, whole_repository: bool) -> Result<(), String> {
    if !whole_repository
        && !git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty()
    {
        return Err("本地已跟踪文件有未提交修改，请先处理本地修改再拉取。".into());
    }
    git(
        root,
        &[
            "merge",
            "--ff-only",
            "--no-overwrite-ignore",
            "--no-autostash",
            reference,
        ],
    )?;
    Ok(())
}

fn sync_repository(
    root: &Path,
    remote: &str,
    branch_name: &str,
    operation: &str,
    whole_repository: bool,
    mut report: impl FnMut(SyncProgress),
) -> Result<String, String> {
    if !matches!(operation, "push" | "pull") {
        return Err("未知同步操作".into());
    }
    let total = if operation == "push" { 6 } else { 4 };
    let mut phase = |step, label| report(SyncProgress { step, total, label });
    phase(1, "检查本地仓库与文件");
    if !root.is_absolute() || !root.is_dir() {
        return Err("请先设置有效的技能库目录".into());
    }
    // 防止在上级源码仓库误执行 add/commit；仅允许技能库自身拥有 .git。
    if !root.join(".git").exists() {
        if git(root, &["rev-parse", "--show-toplevel"]).is_ok() {
            return Err("技能库处于其他 Git 仓库内部，请改用独立技能目录".into());
        }
        git(root, &["init", "-b", branch_name])?;
    }
    let top = git(root, &["rev-parse", "--show-toplevel"])?;
    if !super::known_agents::eq_path(
        &fs::canonicalize(top.trim()).map_err(|e| e.to_string())?,
        &fs::canonicalize(root).map_err(|e| e.to_string())?,
    ) {
        return Err("Git 工作目录与技能库不一致".into());
    }
    let branch = git(root, &["symbolic-ref", "--short", "HEAD"])?;
    if branch.trim() != branch_name {
        return Err(format!(
            "当前分支为 {}，请手动切换到 {} 后同步",
            branch.trim(),
            branch_name
        ));
    }
    if !git(root, &["diff", "--cached", "--name-only"])?
        .trim()
        .is_empty()
    {
        return Err("技能库存在已暂存修改，请先完成本地 Git 操作".into());
    }
    let mut tracked = Vec::new();
    let mut roots = BTreeSet::new();
    if !whole_repository {
        tracked = git(root, &["ls-files", "-z"])?
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        roots = roots_for_paths(&tracked);
        for skill in scan::scan_skills(&root.to_string_lossy()) {
            if skill.healthy {
                roots.insert(skill.key);
            }
        }
        validate_paths(&tracked, &roots)?;
    }
    if !git(root, &["remote"])?.lines().any(|name| name == "origin") {
        git(root, &["remote", "add", "origin", remote])?;
    }
    let check_url = |args: &[&str]| -> Result<(), String> {
        let urls = git(root, args)?;
        let matches = urls.lines().count() == 1 && same_remote(remote, urls.trim());
        if !matches {
            return Err("origin 与设置的仓库不同，请检查 Git 的远端地址、推送地址或 URL 重写规则。软件不会修改现有 origin。".into());
        }
        Ok(())
    };
    check_url(&["remote", "get-url", "--all", "origin"])?;
    if operation == "push" {
        check_url(&["remote", "get-url", "--push", "--all", "origin"])?;
    }
    let reference = format!("refs/heads/{}", branch_name);
    phase(2, "连接远端并查询分支");
    let exists = !git(root, &["ls-remote", "--heads", "origin", &reference])?
        .trim()
        .is_empty();
    let remote_ref = format!("refs/remotes/origin/{}", branch_name);
    let remote_paths = if exists {
        phase(3, "获取远端更新");
        git(
            root,
            &[
                "fetch",
                "--no-tags",
                "origin",
                &format!("{reference}:{remote_ref}"),
            ],
        )?;
        if whole_repository {
            Vec::new()
        } else {
            let paths = tree_paths(root, &remote_ref)?;
            validate_paths(&paths, &roots_for_paths(&paths))?;
            paths
        }
    } else {
        Vec::new()
    };
    let head = git(root, &["rev-parse", "--verify", "HEAD"]).ok();
    if operation == "pull" {
        if !exists {
            return Err("远端分支尚不存在，请先从有技能的电脑推送".into());
        }
        phase(4, "应用远端更新到本地");
        if let Some(head) = &head {
            if git(root, &["merge-base", "--is-ancestor", &remote_ref, "HEAD"]).is_ok() {
                return if head.trim() == git(root, &["rev-parse", &remote_ref])?.trim() {
                    Ok("本机已同步到远端最新版本；本地未提交文件保持原样。".into())
                } else {
                    Ok("本机已包含远端版本，另有本地提交尚未推送。".into())
                };
            }
        }
        fast_forward(root, &remote_ref, whole_repository)?;
        return Ok(if whole_repository {
            "仓库已拉取到本机技能库。"
        } else {
            "技能已拉取到本机技能库；请在 Agent 配置中选择并部署所需技能"
        }
        .into());
    }
    phase(4, "检查并合并可安全应用的远端更新");
    if !whole_repository && roots.is_empty() {
        return Err("没有找到有效技能，未创建提交".into());
    }
    // 首次上传时可接上 GitHub 创建仓库时生成的 README / LICENSE / .gitignore。
    let mut head = head;
    if exists {
        if head.is_none() {
            if !remote_paths.iter().all(|path| repo_metadata(path)) {
                return Err(
                    "远端已有技能。请先拉取，遇到同名文件时用独立技能目录拉取后再合并。".into(),
                );
            }
            fast_forward(root, &remote_ref, whole_repository)?;
            head = Some(git(root, &["rev-parse", "--verify", "HEAD"])?);
        } else if git(root, &["merge-base", "--is-ancestor", &remote_ref, "HEAD"]).is_err() {
            if whole_repository {
                fast_forward(root, &remote_ref, true)?;
                head = Some(git(root, &["rev-parse", "--verify", "HEAD"])?);
            } else {
                return Err(
                    "远端有更新或分支已分叉。请先拉取；若有冲突或分叉，请备份后手动处理。".into(),
                );
            }
        }
    }
    phase(5, "整理本地改动并创建提交");
    let commit_result = (|| {
        if whole_repository {
            git(root, &["add", "-A", "--", "."])?;
        } else {
            stage_skills(root, &roots, tracked, head.is_some())?;
        }
        if !git(root, &["diff", "--cached", "--name-only"])?
            .trim()
            .is_empty()
        {
            git(root, &["commit", "-m", "sync: update shared skills"])?;
        } else if head.is_none() {
            return Err("没有可推送的文件，请检查技能库目录和 Git 忽略规则。".into());
        }
        Ok::<_, String>(())
    })();
    if let Err(error) = commit_result {
        // 入口已确认没有用户暂存的改动。仅恢复索引，不触碰工作区。
        let args = [
            "read-tree",
            head.as_deref().map(str::trim).unwrap_or("--empty"),
        ];
        return Err(match git(root, &args) {
            Ok(_) => error,
            Err(restore) => format!("{error}\n暂存区恢复失败：{restore}"),
        });
    }
    phase(6, "推送到远端，等待服务器确认");
    let result = git(
        root,
        &[
            "-c",
            "remote.origin.mirror=false",
            "push",
            "--porcelain",
            "--no-follow-tags",
            "origin",
            &format!("HEAD:{reference}"),
        ],
    )?;
    if result.lines().any(|line| line.starts_with("=\t")) {
        return Ok("远端已是最新，本机没有待推送的提交。".into());
    }
    Ok(if whole_repository {
        "仓库已推送；另一台电脑可直接拉取。"
    } else {
        "技能已推送到指定 Git 仓库；另一台电脑可使用相同仓库与分支拉取"
    }
    .into())
}

fn stage_skills(
    root: &Path,
    roots: &BTreeSet<String>,
    mut paths: Vec<String>,
    has_head: bool,
) -> Result<(), String> {
    // 分别暂存安全文件与已跟踪的删除项；绝不 add 整个技能库。
    for skill_root in roots {
        let path = root.join(skill_root);
        if path.exists() {
            local_files(&path, root, &mut paths)?;
        }
    }
    let visible: BTreeSet<_> = git(
        root,
        &[
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ],
    )?
    .split('\0')
    .filter(|p| !p.is_empty())
    .map(str::to_owned)
    .collect();
    paths.retain(|path| visible.contains(path));
    paths.sort();
    paths.dedup();
    validate_paths(&paths, roots)?;
    for chunk in paths.chunks(40) {
        let mut args = vec!["add", "-A", "--"];
        let literal: Vec<_> = chunk.iter().map(|p| format!(":(literal){p}")).collect();
        args.extend(literal.iter().map(String::as_str));
        git(root, &args)?;
    }
    let tree = git(root, &["write-tree"])?;
    let staged = tree_paths(root, tree.trim())?;
    let staged_roots = roots_for_paths(&staged);
    validate_paths(&staged, &staged_roots)?;
    if staged_roots.is_empty() && !has_head {
        return Err("没有可推送的技能，请检查技能目录和 Git 忽略规则".into());
    }
    check_staged_credentials(root)
}

#[tauri::command]
pub async fn skill_sync_run(
    state: State<'_, AppState>,
    operation: String,
    on_progress: Channel<SyncProgress>,
) -> Result<String, String> {
    let settings = skill_sync_settings(state.clone())?;
    let root = state.config.lock().unwrap().skills_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = SYNC
            .try_lock()
            .map_err(|_| "已有技能同步正在进行，请稍候")?;
        sync(Path::new(&root), &settings, &operation, |progress| {
            let _ = on_progress.send(progress);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sync_repository(
        root: &Path,
        remote: &str,
        branch: &str,
        operation: &str,
        whole_repository: bool,
    ) -> Result<String, String> {
        super::sync_repository(root, remote, branch, operation, whole_repository, |_| {})
    }
    #[test]
    fn repository_and_branch_reject_credentials_and_options() {
        let settings = |r: &str, b: &str| SyncSettings {
            repository: r.into(),
            branch: b.into(),
            whole_repository: false,
        };
        assert_eq!(
            validate(&settings("alice/skills", "main")).unwrap(),
            "https://github.com/alice/skills.git"
        );
        let ssh = validate(&settings("git@github.com:Alice/skills.git", "master")).unwrap();
        assert!(same_remote("https://github.com/alice/skills.git", &ssh));
        assert!(!same_remote(
            "https://github.com/alice/skills.git",
            "git@github.com:alice/another.git"
        ));
        let old: SyncSettings =
            serde_json::from_str(r#"{"repository":"alice/skills","branch":"main"}"#).unwrap();
        assert!(!old.whole_repository);
        assert!(validate(&settings(
            "https://github.com/alice/skills.git",
            "skills/shared"
        ))
        .is_ok());
        assert!(validate(&settings(
            "https://git.example.test/group/sub/skills.git",
            "main"
        ))
        .is_ok());
        assert!(validate(&settings("git@git.example.test:group/skills.git", "main")).is_ok());
        assert!(validate(&settings(
            "ssh://git@git.example.test:2222/group/skills.git",
            "main"
        ))
        .is_ok());
        for r in [
            "https://token@github.com/a/b",
            "a/../b",
            "file:///tmp/repo",
            "ext::cat /tmp/file",
            "https://alice:secret@git.example.test/team/skills.git",
            "a/-x?token=secret",
        ] {
            assert!(validate(&settings(r, "main")).is_err());
        }
        for b in ["--all", "a..b", "a/.hidden", "a.lock", "a//b"] {
            assert!(validate(&settings("a/b", b)).is_err());
        }
    }
    #[test]
    fn only_skill_files_can_be_shared() {
        let paths = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let safe = paths(&["example/SKILL.md", "example/scripts/run.py", "README.md"]);
        let roots = roots_for_paths(&safe);
        assert!(validate_paths(&safe, &roots).is_ok());
        for p in [
            "example/.env",
            "example/private.key",
            "example/../auth.json",
            "app-config.json",
            "_archive/old/SKILL.md",
        ] {
            assert!(validate_paths(&paths(&[p]), &roots).is_err(), "{p}");
        }
    }

    fn fixture_skill(root: &Path, name: &str) {
        fs::create_dir_all(root.join(name)).unwrap();
        fs::write(
            root.join(name).join("SKILL.md"),
            format!("---\nname: {name}\ndescription: fixture skill\n---\n"),
        )
        .unwrap();
    }

    fn fixture_identity(root: &Path) {
        git(root, &["config", "user.name", "Sync Test"]).unwrap();
        git(root, &["config", "user.email", "sync-test@example.invalid"]).unwrap();
    }

    #[test]
    fn two_computers_round_trip_excludes_secrets_and_rejects_overlap() {
        let tmp = super::super::test_scratch("sync_two_computers");
        let remote = tmp.join("remote.git");
        fs::create_dir_all(&remote).unwrap();
        git(&remote, &["init", "--bare"]).unwrap();
        let remote = remote.to_string_lossy().replace('\\', "/");
        let a = tmp.join("computer-a");
        let b = tmp.join("computer-b");
        for root in [&a, &b] {
            fs::create_dir_all(root).unwrap();
            git(root, &["init", "-b", "main"]).unwrap();
            fixture_identity(root);
        }
        fixture_skill(&a, "example");
        fixture_skill(&a, "外层/再包一层/开发/deep-skill");
        fs::write(a.join("example/.env"), "TOKEN=fixture-only").unwrap();
        fs::write(a.join("example/cert.PFX"), b"fixture-cert").unwrap();
        fs::write(a.join("example/_icon.png"), b"fixture-icon").unwrap();
        fs::create_dir_all(a.join("_archive/old")).unwrap();
        fs::write(a.join("_archive/old/private.txt"), "local-only").unwrap();
        fs::write(a.join("example/notes.txt"), "-----BEGIN PRIVATE KEY-----").unwrap();
        let error = sync_repository(&a, &remote, "main", "push", false).unwrap_err();
        assert!(error.contains("example/notes.txt"));
        assert!(!error.contains("BEGIN PRIVATE KEY"));
        assert!(git(&a, &["ls-files"]).unwrap().is_empty());
        fs::write(a.join("example/notes.txt"), "ordinary documentation").unwrap();
        sync_repository(&a, &remote, "main", "push", false).unwrap();
        fs::write(b.join("local-notes.txt"), "keep local").unwrap();
        sync_repository(&b, &remote, "main", "pull", false).unwrap();
        assert!(b.join("example/SKILL.md").is_file());
        assert!(b.join("外层/再包一层/开发/deep-skill/SKILL.md").is_file());
        assert!(b.join("example/_icon.png").is_file());
        assert!(!b.join("example/.env").exists());
        assert!(!b.join("example/cert.PFX").exists());
        assert!(!b.join("_archive").exists());
        assert_eq!(
            fs::read_to_string(b.join("local-notes.txt")).unwrap(),
            "keep local"
        );
        fixture_skill(&b, "second");
        sync_repository(&b, &remote, "main", "push", false).unwrap();
        sync_repository(&a, &remote, "main", "pull", false).unwrap();
        assert!(a.join("second/SKILL.md").is_file());
        assert!(a.join("example/.env").is_file());
        fixture_skill(&a, "home-new");
        sync_repository(&a, &remote, "main", "push", false).unwrap();
        fixture_skill(&b, "work-new");
        assert!(sync_repository(&b, &remote, "main", "push", false).is_err());
        fs::create_dir_all(b.join("home-new")).unwrap();
        fs::write(b.join("home-new/SKILL.md"), "keep local skill").unwrap();
        fs::write(b.join(".git/info/exclude"), "home-new/\n").unwrap();
        assert!(sync_repository(&b, &remote, "main", "pull", false).is_err());
        assert_eq!(
            fs::read_to_string(b.join("home-new/SKILL.md")).unwrap(),
            "keep local skill"
        );
        fs::rename(b.join("home-new"), b.join("saved-local")).unwrap();
        sync_repository(&b, &remote, "main", "pull", false).unwrap();
        assert!(b.join("work-new/SKILL.md").is_file());
        assert!(b.join("home-new/SKILL.md").is_file());
    }

    #[test]
    fn first_push_preserves_initialized_remote_metadata() {
        let tmp = super::super::test_scratch("sync_initialized_remote");
        let remote = tmp.join("remote");
        let local = tmp.join("local");
        for dir in [&remote, &local] {
            fs::create_dir_all(dir).unwrap();
            git(dir, &["init", "-b", "main"]).unwrap();
            fixture_identity(dir);
        }
        git(
            &remote,
            &["config", "receive.denyCurrentBranch", "updateInstead"],
        )
        .unwrap();
        fs::write(remote.join("README.md"), "initial readme").unwrap();
        git(&remote, &["add", "README.md"]).unwrap();
        git(&remote, &["commit", "-m", "initial readme"]).unwrap();
        let initial = git(&remote, &["rev-parse", "HEAD"]).unwrap();
        fixture_skill(&local, "example");
        sync_repository(
            &local,
            &remote.to_string_lossy().replace('\\', "/"),
            "main",
            "push",
            false,
        )
        .unwrap();
        assert_eq!(git(&remote, &["rev-parse", "HEAD^"]).unwrap(), initial);
        assert!(remote.join("example/SKILL.md").is_file());
        assert_eq!(
            fs::read_to_string(local.join("README.md")).unwrap(),
            "initial readme"
        );
    }

    #[test]
    fn whole_repository_reuses_existing_history_and_git_ignore() {
        let tmp = super::super::test_scratch("sync_whole_repository");
        let server = tmp.join("remote.git");
        let a = tmp.join("computer-a");
        let b = tmp.join("computer-b");
        for dir in [&server, &a, &b] {
            fs::create_dir_all(dir).unwrap();
        }
        git(&server, &["init", "--bare"]).unwrap();
        let remote = server.to_string_lossy().replace('\\', "/");
        for dir in [&a, &b] {
            git(dir, &["init", "-b", "master"]).unwrap();
            fixture_identity(dir);
        }
        fixture_skill(&a, "example");
        fs::create_dir_all(a.join(".claude")).unwrap();
        let settings = ".claude/settings.local.json";
        fs::write(a.join(settings), "{\"fixture\":1}").unwrap();
        fs::write(a.join(".gitignore"), "ignored.txt\n").unwrap();
        fs::write(a.join("ignored.txt"), "local only").unwrap();
        // Mirror an existing repository where the local configuration is already tracked.
        git(&a, &["add", "-f", "--", settings]).unwrap();
        git(&a, &["commit", "-m", "track existing configuration"]).unwrap();
        sync_repository(&a, &remote, "master", "push", true).unwrap();
        assert!(sync_repository(&b, &remote, "master", "pull", false)
            .unwrap_err()
            .contains(settings));
        sync_repository(&b, &remote, "master", "pull", true).unwrap();
        assert!(b.join(settings).is_file());
        assert!(!b.join("ignored.txt").exists());

        fixture_skill(&a, "second");
        let mut steps = Vec::new();
        super::sync_repository(&a, &remote, "master", "push", true, |p| {
            steps.push((p.step, p.total));
        })
        .unwrap();
        assert_eq!(steps, (1..=6).map(|step| (step, 6)).collect::<Vec<_>>());
        fs::write(b.join(settings), "{\"fixture\":2}").unwrap();
        sync_repository(&b, &remote, "master", "push", true).unwrap();
        assert!(b.join("second/SKILL.md").is_file());
        assert!(sync_repository(&b, &remote, "master", "push", true)
            .unwrap()
            .contains("远端已是最新"));

        fs::write(a.join(settings), "{\"fixture\":3}").unwrap();
        steps.clear();
        assert!(
            super::sync_repository(&a, &remote, "master", "pull", true, |p| {
                steps.push((p.step, p.total));
            })
            .is_err()
        );
        assert_eq!(steps, (1..=4).map(|step| (step, 4)).collect::<Vec<_>>());
        assert_eq!(
            fs::read_to_string(a.join(settings)).unwrap(),
            "{\"fixture\":3}"
        );
    }

    #[test]
    fn a_parent_source_repository_is_never_staged() {
        let tmp = super::super::test_scratch("sync_refuse_parent");
        fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "-b", "main"]).unwrap();
        let skills = tmp.join("skills");
        fs::create_dir_all(&skills).unwrap();
        fixture_skill(&skills, "example");
        for whole_repository in [false, true] {
            assert!(sync_repository(
                &skills,
                "https://github.com/example/skills.git",
                "main",
                "push",
                whole_repository,
            )
            .unwrap_err()
            .contains("其他 Git 仓库"));
        }
        assert!(!skills.join(".git").exists());
        assert!(git(&tmp, &["diff", "--cached", "--name-only"])
            .unwrap()
            .is_empty());
    }
}
