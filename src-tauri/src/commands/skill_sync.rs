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
use tauri::{Manager, State};

static SYNC: Mutex<()> = Mutex::new(());

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SyncSettings {
    pub repository: String,
    pub branch: String,
}

fn validate(settings: &SyncSettings) -> Result<String, String> {
    let repository = settings.repository.trim().trim_end_matches('/');
    let repository = repository
        .strip_prefix("https://github.com/")
        .unwrap_or(repository);
    let repository = repository.strip_suffix(".git").unwrap_or(repository);
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
        return Err("请输入 GitHub owner/repo 或 HTTPS 仓库地址；地址中不能包含令牌".into());
    }
    let branch = &settings.branch;
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
    Ok(format!("https://github.com/{repository}.git"))
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
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if start.elapsed() > Duration::from_secs(90) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Git 操作超时；请检查网络和本机 Git 登录状态后重试".into());
            }
            None => std::thread::sleep(Duration::from_millis(40)),
        }
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

fn blocked_part(part: &str) -> bool {
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
                "仓库含非技能或敏感文件，已停止同步：{path}。请使用专用技能仓库"
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

fn local_files(path: &Path, root: &Path, paths: &mut Vec<String>) -> Result<(), String> {
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

fn sync(root: &Path, settings: &SyncSettings, operation: &str) -> Result<String, String> {
    let remote = validate(settings)?;
    if operation == "connect" {
        // 从临时目录发起，首次登录也不需要初始化技能库。
        let dir = std::env::temp_dir();
        run_git(&dir, &["ls-remote", "--heads", &remote], true, false)?;
        return Ok("已连接 GitHub 仓库，当前 Git 账号可读取仓库。推送还需要写入权限。".into());
    }
    sync_repository(root, &remote, &settings.branch, operation)
}

fn fast_forward(root: &Path, reference: &str) -> Result<(), String> {
    if !git(root, &["status", "--porcelain", "--untracked-files=no"])?.is_empty() {
        return Err("本地已跟踪文件有未提交修改，请先处理本地修改再拉取。".into());
    }
    git(
        root,
        &["merge", "--ff-only", "--no-overwrite-ignore", reference],
    )?;
    Ok(())
}

fn sync_repository(
    root: &Path,
    remote: &str,
    branch_name: &str,
    operation: &str,
) -> Result<String, String> {
    if !matches!(operation, "push" | "pull") {
        return Err("未知同步操作".into());
    }
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
    let tracked: Vec<_> = git(root, &["ls-files", "-z"])?
        .split('\0')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    let mut roots = roots_for_paths(&tracked);
    for skill in scan::scan_skills(&root.to_string_lossy()) {
        if skill.healthy {
            roots.insert(skill.key);
        }
    }
    validate_paths(&tracked, &roots)?;
    if !git(root, &["remote"])?.lines().any(|name| name == "origin") {
        git(root, &["remote", "add", "origin", remote])?;
    }
    let check_url = |args: &[&str]| -> Result<(), String> {
        let urls = git(root, args)?;
        let matches = urls.lines().count() == 1
            && urls
                .trim()
                .trim_end_matches(".git")
                .eq_ignore_ascii_case(remote.trim_end_matches(".git"));
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
    let exists = !git(root, &["ls-remote", "--heads", "origin", &reference])?
        .trim()
        .is_empty();
    let remote_ref = format!("refs/remotes/origin/{}", branch_name);
    let remote_paths = if exists {
        git(
            root,
            &[
                "fetch",
                "--no-tags",
                "origin",
                &format!("{reference}:{remote_ref}"),
            ],
        )?;
        let paths = tree_paths(root, &remote_ref)?;
        validate_paths(&paths, &roots_for_paths(&paths))?;
        paths
    } else {
        Vec::new()
    };
    let head = git(root, &["rev-parse", "--verify", "HEAD"]).ok();
    if operation == "pull" {
        if !exists {
            return Err("远端分支尚不存在，请先从有技能的电脑推送".into());
        }
        if let Some(head) = &head {
            if git(root, &["merge-base", "--is-ancestor", &remote_ref, "HEAD"]).is_ok() {
                return if head.trim() == git(root, &["rev-parse", &remote_ref])?.trim() {
                    Ok("本机已同步到远端最新版本；本地未提交文件保持原样。".into())
                } else {
                    Ok("本机已包含远端版本，另有本地提交尚未推送。".into())
                };
            }
        }
        fast_forward(root, &remote_ref)?;
        return Ok("技能已拉取到本机技能库；请在 Agent 配置中选择并部署所需技能".into());
    }
    if roots.is_empty() {
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
            fast_forward(root, &remote_ref)?;
            head = Some(git(root, &["rev-parse", "--verify", "HEAD"])?);
        } else if git(root, &["merge-base", "--is-ancestor", &remote_ref, "HEAD"]).is_err() {
            return Err(
                "远端有更新或分支已分叉。请先拉取；若有冲突或分叉，请备份后手动处理。".into(),
            );
        }
    }
    // 分别暂存安全文件与已跟踪的删除项；绝不 add 整个技能库。
    let mut paths = tracked.clone();
    for skill_root in &roots {
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
    validate_paths(&paths, &roots)?;
    let commit_result = (|| {
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
        if staged_roots.is_empty() && head.is_none() {
            return Err("没有可推送的技能，请检查技能目录和 Git 忽略规则".into());
        }
        check_staged_credentials(root)?;
        if !git(root, &["diff", "--cached", "--name-only"])?
            .trim()
            .is_empty()
        {
            git(root, &["commit", "-m", "sync: update shared skills"])?;
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
    git(
        root,
        &[
            "-c",
            "remote.origin.mirror=false",
            "push",
            "--no-follow-tags",
            "origin",
            &format!("HEAD:{reference}"),
        ],
    )?;
    Ok("技能已推送到指定 GitHub 仓库；另一台电脑可使用相同仓库与分支拉取".into())
}

#[tauri::command]
pub async fn skill_sync_run(
    state: State<'_, AppState>,
    operation: String,
) -> Result<String, String> {
    let settings = skill_sync_settings(state.clone())?;
    let root = state.config.lock().unwrap().skills_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = SYNC
            .try_lock()
            .map_err(|_| "已有技能同步正在进行，请稍候")?;
        sync(Path::new(&root), &settings, &operation)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repository_and_branch_reject_credentials_and_options() {
        let settings = |r: &str, b: &str| SyncSettings {
            repository: r.into(),
            branch: b.into(),
        };
        assert_eq!(
            validate(&settings("alice/skills", "main")).unwrap(),
            "https://github.com/alice/skills.git"
        );
        assert!(validate(&settings(
            "https://github.com/alice/skills.git",
            "skills/shared"
        ))
        .is_ok());
        for r in [
            "https://token@github.com/a/b",
            "a/../b",
            "file:///tmp/repo",
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
        fs::write(a.join("example/.env"), "TOKEN=fixture-only").unwrap();
        fs::write(a.join("example/cert.PFX"), b"fixture-cert").unwrap();
        fs::write(a.join("example/_icon.png"), b"fixture-icon").unwrap();
        fs::create_dir_all(a.join("_archive/old")).unwrap();
        fs::write(a.join("_archive/old/private.txt"), "local-only").unwrap();
        fs::write(a.join("example/notes.txt"), "-----BEGIN PRIVATE KEY-----").unwrap();
        let error = sync_repository(&a, &remote, "main", "push").unwrap_err();
        assert!(error.contains("example/notes.txt"));
        assert!(!error.contains("BEGIN PRIVATE KEY"));
        assert!(git(&a, &["ls-files"]).unwrap().is_empty());
        fs::write(a.join("example/notes.txt"), "ordinary documentation").unwrap();
        sync_repository(&a, &remote, "main", "push").unwrap();
        fs::write(b.join("local-notes.txt"), "keep local").unwrap();
        sync_repository(&b, &remote, "main", "pull").unwrap();
        assert!(b.join("example/SKILL.md").is_file());
        assert!(b.join("example/_icon.png").is_file());
        assert!(!b.join("example/.env").exists());
        assert!(!b.join("example/cert.PFX").exists());
        assert!(!b.join("_archive").exists());
        assert_eq!(
            fs::read_to_string(b.join("local-notes.txt")).unwrap(),
            "keep local"
        );
        fixture_skill(&b, "second");
        sync_repository(&b, &remote, "main", "push").unwrap();
        sync_repository(&a, &remote, "main", "pull").unwrap();
        assert!(a.join("second/SKILL.md").is_file());
        assert!(a.join("example/.env").is_file());
        fixture_skill(&a, "home-new");
        sync_repository(&a, &remote, "main", "push").unwrap();
        fixture_skill(&b, "work-new");
        assert!(sync_repository(&b, &remote, "main", "push").is_err());
        fs::create_dir_all(b.join("home-new")).unwrap();
        fs::write(b.join("home-new/SKILL.md"), "keep local skill").unwrap();
        fs::write(b.join(".git/info/exclude"), "home-new/\n").unwrap();
        assert!(sync_repository(&b, &remote, "main", "pull").is_err());
        assert_eq!(
            fs::read_to_string(b.join("home-new/SKILL.md")).unwrap(),
            "keep local skill"
        );
        fs::rename(b.join("home-new"), b.join("saved-local")).unwrap();
        sync_repository(&b, &remote, "main", "pull").unwrap();
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
    fn a_parent_source_repository_is_never_staged() {
        let tmp = super::super::test_scratch("sync_refuse_parent");
        fs::create_dir_all(&tmp).unwrap();
        git(&tmp, &["init", "-b", "main"]).unwrap();
        let skills = tmp.join("skills");
        fs::create_dir_all(&skills).unwrap();
        fixture_skill(&skills, "example");
        assert!(sync_repository(
            &skills,
            "https://github.com/example/skills.git",
            "main",
            "push"
        )
        .unwrap_err()
        .contains("其他 Git 仓库"));
        assert!(!skills.join(".git").exists());
        assert!(git(&tmp, &["diff", "--cached", "--name-only"])
            .unwrap()
            .is_empty());
    }
}
