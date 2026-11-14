//! Encrypted skill snapshots. Storage providers never receive the passphrase.
use super::{hidden_command, scan, skill_sync, AppState};
use aes_gcm::{aead::Aead, Aes256Gcm, KeyInit, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine};
use quick_xml::{events::Event, Reader};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
};
use tauri::{Manager, State};
use url::Url;
use zeroize::{Zeroize, Zeroizing};

const MAGIC: &[u8] = b"ASHBACKUP\x01";
const PREFIX: &str = "agentskillhub-";
const EXT: &str = ".ashbackup";
const MAX_FILES: usize = 12_000;
const MAX_SOURCE: usize = 48 * 1024 * 1024;
const MAX_ARCHIVE: usize = 72 * 1024 * 1024;
const MAX_LIST: usize = 2 * 1024 * 1024;

#[derive(Clone, Deserialize, Serialize, Default)]
pub struct Target {
    pub location: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub webdav: bool,
}

#[derive(Deserialize, Serialize)]
struct Entry {
    path: String,
    data: String,
}
impl Drop for Entry {
    fn drop(&mut self) {
        self.data.zeroize();
    }
}

#[derive(Serialize)]
pub struct BackupItem {
    name: String,
    created: u64,
}

#[derive(Serialize)]
pub struct BackupResult {
    name: String,
    path: String,
    files: usize,
}

fn random<const N: usize>() -> Result<[u8; N], String> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes).map_err(|_| "系统随机数不可用")?;
    Ok(bytes)
}

fn backup_name() -> Result<String, String> {
    let nonce: String = random::<12>()?.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("{PREFIX}{}-{nonce}{EXT}", super::now_ts()))
}

fn item(name: &str) -> Option<BackupItem> {
    let (created, nonce) = name
        .strip_prefix(PREFIX)?
        .strip_suffix(EXT)?
        .split_once('-')?;
    if created.len() != 10
        || !created.bytes().all(|b| b.is_ascii_digit())
        || nonce.len() != 24
        || !nonce.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return None;
    }
    Some(BackupItem {
        name: name.into(),
        created: created.parse().ok()?,
    })
}

fn cipher(passphrase: &str, salt: &[u8]) -> Result<Aes256Gcm, String> {
    if passphrase.chars().count() < 12 || passphrase.len() > 1024 {
        return Err("备份口令至少需要 12 个字符，最多 1024 字节".into());
    }
    // Format v1 pins the KDF settings so snapshots remain readable across upgrades.
    let params = argon2::Params::new(19_456, 2, 1, Some(32)).map_err(|e| e.to_string())?;
    let kdf = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut key = Zeroizing::new([0u8; 32]);
    kdf.hash_password_into(passphrase.as_bytes(), salt, key.as_mut())
        .map_err(|_| "无法生成备份加密密钥")?;
    Aes256Gcm::new_from_slice(key.as_ref()).map_err(|_| "无法初始化备份加密".into())
}

fn seal(entries: &[Entry], passphrase: &str) -> Result<Vec<u8>, String> {
    let salt = random::<16>()?;
    let nonce = random::<12>()?;
    let plain = Zeroizing::new(serde_json::to_vec(entries).map_err(|e| e.to_string())?);
    let mut output = MAGIC.to_vec();
    output.extend(salt);
    output.extend(nonce);
    let encrypted = cipher(passphrase, &salt)?
        .encrypt(Nonce::from_slice(&nonce), plain.as_slice())
        .map_err(|_| "备份加密失败")?;
    output.extend(encrypted);
    if output.len() > MAX_ARCHIVE {
        return Err("加密快照超过 72 MiB".into());
    }
    Ok(output)
}

fn unseal(bytes: &[u8], passphrase: &str) -> Result<Vec<Entry>, String> {
    let body = bytes
        .strip_prefix(MAGIC)
        .filter(|data| data.len() >= 44 && bytes.len() <= MAX_ARCHIVE)
        .ok_or("不是有效的 AgentSkillHub 加密快照")?;
    let plain = Zeroizing::new(
        cipher(passphrase, &body[..16])?
            .decrypt(Nonce::from_slice(&body[16..28]), &body[28..])
            .map_err(|_| "口令不正确，或快照已损坏")?,
    );
    serde_json::from_slice(&plain).map_err(|_| "快照内容格式无效".into())
}

fn checked_part(part: &str) -> bool {
    let upper = part
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    !part.is_empty()
        && !part.ends_with([' ', '.'])
        && !skill_sync::blocked_part(part)
        && !part
            .chars()
            .any(|c| c.is_control() || r#"\:<>"/|?*"#.contains(c))
        && !matches!(
            upper.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        )
        && !["COM", "LPT"].iter().any(|p| {
            upper.strip_prefix(p).is_some_and(|n| {
                matches!(
                    n,
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        })
}

fn validate_entries(entries: &[Entry]) -> Result<(), String> {
    if entries.is_empty() || entries.len() > MAX_FILES {
        return Err("快照文件数量无效".into());
    }
    let mut files = HashSet::new();
    let mut roots = BTreeSet::new();
    for entry in entries {
        if entry.path.len() > 1024
            || !entry.path.split('/').all(checked_part)
            || !files.insert(entry.path.to_lowercase())
        {
            return Err(format!("快照包含无效或重复路径：{}", entry.path));
        }
        if let Some(root) = entry.path.strip_suffix("/SKILL.md") {
            roots.insert(root);
        }
    }
    for entry in entries {
        let mut prefixes = entry.path.match_indices('/').map(|(i, _)| &entry.path[..i]);
        if prefixes
            .clone()
            .any(|prefix| files.contains(&prefix.to_lowercase()))
            || !prefixes.any(|prefix| roots.contains(prefix))
        {
            return Err(format!("文件不在有效技能目录内：{}", entry.path));
        }
    }
    Ok(())
}

fn collect(root: &Path) -> Result<Vec<Entry>, String> {
    let root = root
        .canonicalize()
        .map_err(|_| "请先设置有效的技能库目录")?;
    let mut paths = Vec::new();
    for skill in scan::scan_skills(&root.to_string_lossy()) {
        if !skill.key.split('/').all(checked_part) {
            continue;
        }
        // Check every wrapper, not only the first category: deeper links must
        // not allow backup traversal to escape the configured library.
        let mut dir = root.clone();
        for part in Path::new(&skill.key).parent().into_iter().flat_map(|p| p.components()) {
            dir.push(part);
            if super::deploy::is_junction(&dir) || dir.canonicalize().map_err(|e| e.to_string())? != dir {
                return Err(format!("分类目录是外部链接：{}", dir.display()));
            }
        }
        skill_sync::local_files(&root.join(skill.key), &root, &mut paths)?;
    }
    paths.sort();
    paths.dedup();
    if paths.len() > MAX_FILES {
        return Err("一次最多备份 12000 个文件".into());
    }
    let mut total = 0;
    let mut entries = Vec::new();
    for path in paths {
        let full = root.join(&path);
        if full.canonicalize().map_err(|e| e.to_string())? != full {
            return Err(format!("备份不跟随外部链接：{path}"));
        }
        let remaining = MAX_SOURCE - total;
        let mut content = Zeroizing::new(Vec::new());
        fs::File::open(full)
            .map_err(|e| e.to_string())?
            .take((remaining + 1) as u64)
            .read_to_end(&mut content)
            .map_err(|e| e.to_string())?;
        if content.len() > remaining {
            return Err("技能文件总量超过 48 MiB，未创建快照".into());
        }
        total += content.len();
        entries.push(Entry {
            path,
            data: STANDARD.encode(&*content),
        });
    }
    if entries.is_empty() {
        return Err("当前技能库没有可备份的技能文件".into());
    }
    validate_entries(&entries)?;
    Ok(entries)
}

// Only this application-owned, freshly-created workspace is cleaned up.
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn scratch(parent: &Path) -> Result<Scratch, String> {
    let dir = parent.join(format!(".{}.partial", backup_name()?));
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    Ok(Scratch(dir))
}

fn restore(root: &Path, entries: &[Entry]) -> Result<PathBuf, String> {
    validate_entries(entries)?;
    if !root.is_absolute() {
        return Err("请先设置技能库绝对路径".into());
    }
    let parent_path = root.parent().ok_or("技能库目录没有父目录")?;
    fs::create_dir_all(parent_path).map_err(|e| format!("无法创建技能库的上一级目录：{e}"))?;
    let parent = parent_path
        .canonicalize()
        .map_err(|_| "技能库的上一级目录不存在")?;
    let workspace = scratch(&parent)?;
    let content = workspace.0.join("skills");
    fs::create_dir(&content).map_err(|e| e.to_string())?;
    let mut total = 0;
    for entry in entries {
        let bytes = Zeroizing::new(
            STANDARD
                .decode(&entry.data)
                .map_err(|_| "快照文件数据无效")?,
        );
        total += bytes.len();
        if total > MAX_SOURCE {
            return Err("快照内容超过 48 MiB".into());
        }
        let file = content.join(&entry.path);
        fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(file)
            .and_then(|mut f| f.write_all(&bytes))
            .map_err(|e| e.to_string())?;
    }
    let name = backup_name()?.replace(EXT, "-restored");
    let restored = parent.join(&name);
    fs::rename(&content, &restored).map_err(|e| e.to_string())?;
    Ok(parent_path.join(name))
}

fn endpoint(value: &str) -> Result<Url, String> {
    let mut url = Url::parse(value.trim()).map_err(|_| "请输入完整的 WebDAV 文件夹地址")?;
    let secure = url.scheme() == "https"
        || (cfg!(test) && url.scheme() == "http" && url.host_str() == Some("127.0.0.1"));
    if !secure
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("WebDAV 地址必须使用 HTTPS，地址中不要包含账号、密码或查询参数".into());
    }
    if !url.path().ends_with('/') {
        url.path_segments_mut()
            .map_err(|_| "无效的 WebDAV 路径")?
            .push("");
    }
    Ok(url)
}

fn local_dir(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value.trim());
    if !path.is_absolute() || !path.is_dir() {
        return Err("请选择一个已存在的备份文件夹".into());
    }
    path.canonicalize().map_err(|e| e.to_string())
}

fn dav(
    target: &Target,
    password: &str,
    method: &str,
    name: Option<&str>,
    upload: Option<&Path>,
) -> Result<Vec<u8>, String> {
    let base = endpoint(&target.location)?;
    let url = if let Some(name) = name {
        base.join(name).map_err(|e| e.to_string())?
    } else {
        base
    };
    if target.username.contains(':') || target.username.len() + password.len() > 8192 {
        return Err("WebDAV 登录信息格式无效".into());
    }
    let auth = Zeroizing::new(if target.username.is_empty() && password.is_empty() {
        String::new()
    } else {
        format!(
            "header = \"Authorization: Basic {}\"\n",
            STANDARD.encode(format!("{}:{password}", target.username))
        )
    });
    let max = if method == "GET" {
        MAX_ARCHIVE
    } else {
        MAX_LIST
    };
    let workspace = scratch(&std::env::temp_dir())?;
    let response = workspace.0.join("response");
    let mut command = hidden_command("curl");
    command
        .args([
            "-q",
            "--config",
            "-",
            "--silent",
            "--show-error",
            "--globoff",
            "--connect-timeout",
            "12",
            "--max-time",
            "120",
            "--max-filesize",
            &max.to_string(),
            "--proto",
            if url.scheme() == "https" {
                "=https"
            } else {
                "=http"
            },
            "--request",
            method,
            "--write-out",
            "%{http_code}",
            "--url",
            url.as_str(),
        ])
        .arg("--output")
        .arg(&response);
    if method == "PROPFIND" {
        command.args(["--header", "Depth: 1"]);
    }
    if let Some(path) = upload {
        command
            .args([
                "--header",
                "If-None-Match: *",
                "--header",
                "Content-Type: application/octet-stream",
                "--upload-file",
            ])
            .arg(path);
    }
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "无法启动系统 curl")?;
    let sent = child.stdin.take().unwrap().write_all(auth.as_bytes());
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    sent.map_err(|_| "WebDAV 请求未能启动")?;
    if !output.status.success() {
        return Err(format!(
            "WebDAV 连接或传输失败（curl {}）",
            output.status.code().unwrap_or(-1)
        ));
    }
    let code = std::str::from_utf8(&output.stdout).unwrap_or("").trim();
    let accepted = match method {
        "PROPFIND" => code == "207",
        "GET" => code == "200",
        "PUT" => matches!(code, "200" | "201" | "204"),
        _ => false,
    };
    if !accepted {
        return Err(match code {
            "401" | "403" => "WebDAV 账号、应用密码或目录权限不正确".into(),
            "404" | "409" => "WebDAV 目录或快照不存在，请确认服务器上的目录已创建".into(),
            "301" | "302" | "307" | "308" => {
                "服务器要求跳转，请填写最终的 WebDAV 文件夹地址".into()
            }
            _ => format!("WebDAV 请求失败（HTTP {code}）"),
        });
    }
    if method == "PUT" {
        return Ok(Vec::new());
    }
    let mut body = Vec::new();
    fs::File::open(response)
        .map_err(|e| e.to_string())?
        .take((max + 1) as u64)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() > max {
        return Err("WebDAV 响应超过大小限制".into());
    }
    Ok(body)
}

fn dav_names(bytes: &[u8], base: &Url) -> Result<Vec<BackupItem>, String> {
    let mut xml = Reader::from_reader(bytes);
    let mut result = Vec::new();
    let mut in_href = false;
    loop {
        match xml
            .read_event()
            .map_err(|_| "WebDAV 目录响应不是有效 XML")?
        {
            Event::Start(tag) if tag.local_name().as_ref() == "href" => in_href = true,
            Event::End(tag) if tag.local_name().as_ref() == "href" => in_href = false,
            Event::Text(text) if in_href => {
                if let Ok(url) = base.join(text.into_inner().trim()) {
                    if url.origin() == base.origin() {
                        if let Some(name) = url.path().strip_prefix(base.path()).and_then(item) {
                            result.push(name);
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(result)
}

fn list(target: &Target, password: &str) -> Result<Vec<BackupItem>, String> {
    let mut result: Vec<_> = if target.webdav {
        dav_names(
            &dav(target, password, "PROPFIND", None, None)?,
            &endpoint(&target.location)?,
        )?
    } else {
        fs::read_dir(local_dir(&target.location)?)
            .map_err(|e| e.to_string())?
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_file()))
            .filter_map(|entry| item(&entry.file_name().to_string_lossy()))
            .collect()
    };
    result.sort_by(|a, b| b.name.cmp(&a.name));
    result.dedup_by(|a, b| a.name == b.name);
    Ok(result)
}

fn config_file(state: &AppState) -> Result<PathBuf, String> {
    let dir = state
        .handle
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("skill-backup.json"))
}

#[tauri::command]
pub async fn pick_backup_folder() -> Result<String, String> {
    #[cfg(windows)]
    return tauri::async_runtime::spawn_blocking(|| {
        let output = hidden_command("powershell.exe")
            .args([
                "-NoProfile",
                "-STA",
                "-Command",
                r#"
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
Add-Type -AssemblyName System.Windows.Forms
$dialog = New-Object System.Windows.Forms.FolderBrowserDialog
$dialog.Description = 'AgentSkillHub'
try {
  if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) {
    [Console]::Write($dialog.SelectedPath)
  }
} finally { $dialog.Dispose() }
"#,
            ])
            .output()
            .map_err(|_| "无法打开文件夹选择器")?;
        if !output.status.success() {
            return Err("无法打开文件夹选择器".into());
        }
        String::from_utf8(output.stdout).map_err(|_| "无法读取选择的文件夹".into())
    })
    .await
    .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    Err("请直接填写备份文件夹路径".into())
}

#[tauri::command]
pub fn skill_backup_target(state: State<'_, AppState>) -> Result<Target, String> {
    match fs::read(config_file(&state)?) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| e.to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Target::default()),
        Err(e) => Err(e.to_string()),
    }
}

#[tauri::command]
pub async fn skill_backup_list(
    state: State<'_, AppState>,
    target: Target,
    password: String,
) -> Result<Vec<BackupItem>, String> {
    let config = config_file(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let password = Zeroizing::new(password);
        let result = list(&target, &password)?;
        super::mcp::write_recoverable(
            &config,
            &serde_json::to_vec(&target).map_err(|e| e.to_string())?,
        )?;
        Ok(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn skill_backup_create(
    state: State<'_, AppState>,
    target: Target,
    password: String,
    passphrase: String,
) -> Result<BackupResult, String> {
    let root = PathBuf::from(state.config.lock().unwrap().skills_root.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = skill_sync::SYNC
            .try_lock()
            .map_err(|_| "已有备份或同步正在进行")?;
        let password = Zeroizing::new(password);
        let passphrase = Zeroizing::new(passphrase);
        let entries = collect(&root)?;
        let bytes = seal(&entries, &passphrase)?;
        let name = backup_name()?;
        let path = if target.webdav {
            let workspace = scratch(&std::env::temp_dir())?;
            let file = workspace.0.join(&name);
            fs::write(&file, bytes).map_err(|e| e.to_string())?;
            dav(&target, &password, "PUT", Some(&name), Some(&file))?;
            String::new()
        } else {
            let directory = local_dir(&target.location)?;
            let workspace = scratch(&directory)?;
            let pending = workspace.0.join(&name);
            fs::write(&pending, bytes).map_err(|e| e.to_string())?;
            let file = directory.join(&name);
            fs::rename(pending, &file).map_err(|e| e.to_string())?;
            file.to_string_lossy().into_owned()
        };
        Ok(BackupResult {
            name,
            path,
            files: entries.len(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn skill_backup_restore(
    state: State<'_, AppState>,
    target: Target,
    password: String,
    passphrase: String,
    name: String,
) -> Result<BackupResult, String> {
    let root = PathBuf::from(state.config.lock().unwrap().skills_root.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = skill_sync::SYNC
            .try_lock()
            .map_err(|_| "已有备份或同步正在进行")?;
        let password = Zeroizing::new(password);
        let passphrase = Zeroizing::new(passphrase);
        item(&name).ok_or("快照名称无效")?;
        let bytes = if target.webdav {
            dav(&target, &password, "GET", Some(&name), None)?
        } else {
            let path = local_dir(&target.location)?.join(&name);
            let mut bytes = Vec::new();
            if !fs::symlink_metadata(&path)
                .map_err(|e| e.to_string())?
                .is_file()
            {
                return Err("快照不是普通文件".into());
            }
            fs::File::open(path)
                .map_err(|e| e.to_string())?
                .take((MAX_ARCHIVE + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            bytes
        };
        let entries = unseal(&bytes, &passphrase)?;
        let path = restore(&root, &entries)?;
        Ok(BackupResult {
            name,
            path: path.to_string_lossy().into_owned(),
            files: entries.len(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        net::TcpListener,
        time::Duration,
    };

    #[test]
    #[cfg(windows)]
    fn encrypted_snapshot_rejects_links_in_deep_wrapper_directories() {
        let tmp = super::super::test_scratch("backup_deep_link");
        let root = tmp.join("library");
        let outside = tmp.join("outside");
        fs::create_dir_all(root.join("外层/内层")).unwrap();
        fs::create_dir_all(outside.join("demo")).unwrap();
        fs::write(outside.join("demo/SKILL.md"), "---\nname: demo\n---").unwrap();
        let link = root.join("外层/内层/链接");
        super::super::deploy::create_junction(&link, &outside).unwrap();
        assert!(collect(&root).err().unwrap().contains("链接"));
        let mut paths = Vec::new();
        assert!(skill_sync::local_files(&link.join("demo"), &root, &mut paths).is_err());
        super::super::deploy::remove_junction(&link).unwrap();
        fs::remove_dir_all(tmp).unwrap();
    }

    #[test]
    fn encrypted_snapshot_round_trip_preserves_live_library() {
        let parent = super::super::test_scratch("encrypted_backup");
        let root = parent.join("current");
        let skill = root.join("外层/再包一层/分类/样例");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# fixture skill").unwrap();
        fs::write(skill.join("data.bin"), [0, 255, 26, 10]).unwrap();
        fs::write(skill.join(".env"), "fixture secret").unwrap();
        let entries = collect(&root).unwrap();
        assert_eq!(entries.len(), 2);
        let pass = "fixture-long-passphrase";
        let sealed = seal(&entries, pass).unwrap();
        assert!(unseal(&sealed, "different-passphrase").is_err());
        let mut corrupted = sealed.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        assert!(unseal(&corrupted, pass).is_err());
        let restored = restore(&root, &unseal(&sealed, pass).unwrap()).unwrap();
        assert_eq!(
            fs::read(restored.join("外层/再包一层/分类/样例/data.bin")).unwrap(),
            [0, 255, 26, 10]
        );
        assert!(!restored.join("外层/再包一层/分类/样例/.env").exists());
        assert!(skill.join(".env").exists());
        let first_run = parent.join("new-computer/skills");
        let restored = restore(&first_run, &unseal(&sealed, pass).unwrap()).unwrap();
        assert!(restored.join("外层/再包一层/分类/样例/SKILL.md").is_file());
        assert!(!first_run.exists());
        for invalid in [
            "../SKILL.md",
            "foo/AUX.txt",
            "foo/.git/config",
            "foo/stream:config",
        ] {
            assert!(validate_entries(&[
                Entry {
                    path: "foo/SKILL.md".into(),
                    data: String::new()
                },
                Entry {
                    path: invalid.into(),
                    data: String::new()
                },
            ])
            .is_err());
        }
    }

    #[test]
    fn webdav_upload_list_and_download_use_the_same_encrypted_bytes() {
        let server = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let name = backup_name().unwrap();
        let content = vec![0x1A, 0, 255, 10, 13];
        let content_in = content.clone();
        let name_in = name.clone();
        let handler = std::thread::spawn(move || {
            for method in ["PUT", "PROPFIND", "GET"] {
                let (stream, _) = server.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut stream = BufReader::new(stream);
                let mut request = String::new();
                stream.read_line(&mut request).unwrap();
                let path = if method == "PROPFIND" {
                    "/skills/".into()
                } else {
                    format!("/skills/{name_in}")
                };
                assert!(request.starts_with(&format!("{method} {path} ")));
                let mut headers = Vec::new();
                loop {
                    let mut line = String::new();
                    assert_ne!(stream.read_line(&mut line).unwrap(), 0);
                    if line == "\r\n" {
                        break;
                    }
                    headers.push(line.trim().to_ascii_lowercase());
                }
                assert!(headers
                    .iter()
                    .any(|line| line.starts_with("authorization: basic ")));
                let (status, response) = match method {
                    "PUT" => {
                        assert!(headers.iter().any(|line| line == "if-none-match: *"));
                        if headers.iter().any(|line| line == "expect: 100-continue") {
                            stream
                                .get_mut()
                                .write_all(b"HTTP/1.1 100 Continue\r\n\r\n")
                                .unwrap();
                        }
                        let mut body = vec![0; content_in.len()];
                        stream.read_exact(&mut body).unwrap();
                        assert_eq!(body, content_in);
                        ("201 Created", vec![])
                    }
                    "PROPFIND" => {
                        assert!(headers.iter().any(|line| line == "depth: 1"));
                        let xml = format!("<D:multistatus xmlns:D=\"DAV:\"><D:response><D:href>/skills/{name_in}</D:href></D:response><D:response><D:href>/another/{name_in}</D:href></D:response></D:multistatus>");
                        ("207 Multi-Status", xml.into_bytes())
                    }
                    _ => ("200 OK", content_in.clone()),
                };
                write!(
                    stream.get_mut(),
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    response.len()
                )
                .unwrap();
                stream.get_mut().write_all(&response).unwrap();
            }
        });
        let tmp = scratch(&std::env::temp_dir()).unwrap();
        let file = tmp.0.join(&name);
        fs::write(&file, &content).unwrap();
        let target = Target {
            location: format!("http://{addr}/skills/"),
            username: "fixture".into(),
            webdav: true,
        };
        dav(&target, "test-password", "PUT", Some(&name), Some(&file)).unwrap();
        let items = list(&target, "test-password").unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, name);
        assert_eq!(
            dav(&target, "test-password", "GET", Some(&name), None).unwrap(),
            content
        );
        handler.join().unwrap();
    }
}
