//! 只检查本项目的正式 Release；下载由用户在浏览器中完成，不替换运行中的程序。
use super::{hidden_command, AppState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::PathBuf, sync::Mutex};
use tauri::{Manager, State};

const REPOSITORY: &str = "fysh1010/AgentSkillHub";
const CURRENT: &str = env!("CARGO_PKG_VERSION");
static CHECK: Mutex<()> = Mutex::new(());

#[derive(Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub available: bool,
    pub release_url: String,
    pub download_url: String,
    pub notes: String,
    pub checked_at: u64,
}

#[derive(Serialize, Deserialize)]
pub struct UpdateSettings {
    pub check_on_start: bool,
}

fn app_path(state: &AppState, name: &str) -> Result<PathBuf, String> {
    let dir = state
        .handle
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(name))
}

#[tauri::command]
pub fn update_settings(state: State<'_, AppState>) -> Result<UpdateSettings, String> {
    let path = app_path(&state, "update-settings.json")?;
    if !path.exists() {
        return Ok(UpdateSettings {
            check_on_start: true,
        });
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_update_settings(
    state: State<'_, AppState>,
    settings: UpdateSettings,
) -> Result<(), String> {
    super::mcp::write_recoverable(
        &app_path(&state, "update-settings.json")?,
        &serde_json::to_vec(&settings).map_err(|e| e.to_string())?,
    )
}

fn version(value: &str) -> Option<[u64; 3]> {
    let parts = value
        .trim()
        .trim_start_matches('v')
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    parts.try_into().ok()
}

fn official_url(value: &str, download: bool) -> bool {
    let prefix = if download {
        format!("https://github.com/{REPOSITORY}/releases/download/")
    } else {
        format!("https://github.com/{REPOSITORY}/releases/tag/")
    };
    let Some(path) = value.strip_prefix(&prefix) else {
        return false;
    };
    if download {
        let Some((tag, asset)) = path.split_once('/') else {
            return false;
        };
        version(tag).is_some()
            && asset.ends_with("-portable.zip")
            && asset
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
    } else {
        version(path).is_some()
            && path
                .bytes()
                .all(|c| c.is_ascii_digit() || b"v.".contains(&c))
    }
}

fn parse_release(value: &Value, now: u64) -> Result<UpdateInfo, String> {
    if value["draft"].as_bool() != Some(false) || value["prerelease"].as_bool() != Some(false) {
        return Err("该 Release 不是正式发布版本".into());
    }
    let latest = value["tag_name"].as_str().ok_or("Release 缺少版本号")?;
    let latest_version = version(latest).ok_or("Release 版本号不符合 v主版本.次版本.补丁 格式")?;
    let release_url = value["html_url"]
        .as_str()
        .filter(|url| official_url(url, false))
        .ok_or("Release 地址不是本项目官方地址")?;
    let download = value["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|asset| {
            let name = asset["name"].as_str()?.to_ascii_lowercase();
            let url = asset["browser_download_url"].as_str()?;
            (name.ends_with("-portable.zip") && official_url(url, true)).then(|| url.to_owned())
        })
        .unwrap_or_default();
    Ok(UpdateInfo {
        current_version: CURRENT.into(),
        latest_version: latest.into(),
        available: latest_version > version(CURRENT).ok_or("客户端版本号无效")?,
        release_url: release_url.into(),
        download_url: download,
        notes: value["body"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(6000)
            .collect(),
        checked_at: now,
    })
}

#[tauri::command]
pub async fn check_app_update(
    state: State<'_, AppState>,
    force: bool,
) -> Result<UpdateInfo, String> {
    let cache = app_path(&state, "update-check.json")?;
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = CHECK.try_lock().map_err(|_| "正在检查版本，请稍候")?;
        let now = super::now_ts();
        if !force {
            if let Ok(bytes) = fs::read(&cache) {
                if let Ok(info) = serde_json::from_slice::<UpdateInfo>(&bytes) {
                    if info.current_version == CURRENT
                        && now.saturating_sub(info.checked_at) < 24 * 3600
                        && official_url(&info.release_url, false)
                        && (info.download_url.is_empty() || official_url(&info.download_url, true))
                    {
                        return Ok(info);
                    }
                }
            }
        }
        let output = hidden_command("curl")
            .args([
                "-q", // 不读用户 curlrc，避免其改变输出格式或目标。
                "--silent",
                "--show-error",
                "--max-time",
                "20",
                "--proto",
                "=https",
                "--write-out",
                "\n%{http_code}",
                "-H",
                "Accept: application/vnd.github+json",
                "-H",
                "User-Agent: AgentSkillHub",
                &format!("https://api.github.com/repos/{REPOSITORY}/releases/latest"),
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("无法读取 GitHub 最新版本，请检查网络或稍后重试".into());
        }
        let response =
            String::from_utf8(output.stdout).map_err(|_| "GitHub 响应不是有效的 UTF-8")?;
        let (body, status) = response
            .rsplit_once('\n')
            .ok_or("无法读取 GitHub 响应状态")?;
        match status {
            "200" => (),
            "403" | "429" => {
                return Err(
                    "GitHub 暂时拒绝版本查询，可能已达到请求限额。请稍后重试，或打开官方发布页。"
                        .into(),
                )
            }
            "404" => return Err("GitHub 未返回正式发布版本。可打开官方发布页查看。".into()),
            _ => {
                return Err(format!(
                    "GitHub 版本查询失败（HTTP {status}），可打开官方发布页查看。"
                ))
            }
        }
        let value: Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
        let info = parse_release(&value, now)?;
        super::mcp::write_recoverable(
            &cache,
            &serde_json::to_vec(&info).map_err(|e| e.to_string())?,
        )?;
        Ok(info)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn open_update_link(url: String) -> Result<(), String> {
    if url != format!("https://github.com/{REPOSITORY}/releases/latest")
        && !official_url(&url, false)
        && !official_url(&url, true)
    {
        return Err("只允许打开本项目官方 Release 或下载地址".into());
    }
    #[cfg(windows)]
    hidden_command("rundll32.exe")
        .args(["url.dll,FileProtocolHandler", &url])
        .spawn()
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    return Err("当前仅支持 Windows 上打开下载链接".into());
    #[allow(unreachable_code)]
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compares_numeric_versions_and_rejects_prereleases() {
        assert!(version("v0.10.0") > version("0.9.9"));
        assert!(version("v1.0.0-beta").is_none());
        assert!(version("garbage").is_none());
        let mut value = serde_json::json!({"draft":false,"prerelease":false,"tag_name":"v999.0.0",
            "html_url":"https://github.com/fysh1010/AgentSkillHub/releases/tag/v999.0.0",
            "assets":[{"name":"AgentSkillHub-v999.0.0-portable.zip","browser_download_url":"https://evil.example/app.zip"}]});
        let info = parse_release(&value, 42).unwrap();
        assert!(info.available);
        assert!(info.download_url.is_empty());
        assert!(!official_url(
            "https://github.com/fysh1010/AgentSkillHub/releases/tag/../../",
            false
        ));
        value["assets"][0]["browser_download_url"] = "https://github.com/fysh1010/AgentSkillHub/releases/download/v999.0.0/AgentSkillHub-v999.0.0-portable.zip".into();
        assert!(!parse_release(&value, 42).unwrap().download_url.is_empty());
        value["prerelease"] = true.into();
        assert!(parse_release(&value, 42).is_err());
    }
}
