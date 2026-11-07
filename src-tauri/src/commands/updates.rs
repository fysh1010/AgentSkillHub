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

fn release_info(release_url: &str, now: u64) -> Result<UpdateInfo, String> {
    let prefix = format!("https://github.com/{REPOSITORY}/releases/tag/");
    let latest = release_url
        .strip_prefix(&prefix)
        .filter(|_| official_url(release_url, false))
        .ok_or("Release 地址不是本项目的正式版本地址")?;
    let latest_version = version(latest).ok_or("Release 版本号无效")?;
    Ok(UpdateInfo {
        current_version: CURRENT.into(),
        latest_version: latest.into(),
        available: latest_version > version(CURRENT).ok_or("客户端版本号无效")?,
        release_url: release_url.into(),
        download_url: String::new(),
        notes: String::new(),
        checked_at: now,
    })
}

fn parse_release(value: &Value, now: u64) -> Result<UpdateInfo, String> {
    if value["draft"].as_bool() != Some(false) || value["prerelease"].as_bool() != Some(false) {
        return Err("该 Release 不是正式发布版本".into());
    }
    let latest = value["tag_name"].as_str().ok_or("Release 缺少版本号")?;
    let release_url = value["html_url"]
        .as_str()
        .ok_or("Release 地址不是本项目官方地址")?;
    let mut info = release_info(release_url, now)?;
    if latest != info.latest_version {
        return Err("Release 标签与发布地址不一致".into());
    }
    info.download_url = value["assets"]
        .as_array()
        .into_iter()
        .flatten()
        .find_map(|asset| {
            let name = asset["name"].as_str()?.to_ascii_lowercase();
            let url = asset["browser_download_url"].as_str()?;
            (name.ends_with("-portable.zip") && official_url(url, true)).then(|| url.to_owned())
        })
        .unwrap_or_default();
    info.notes = value["body"]
        .as_str()
        .unwrap_or("")
        .chars()
        .take(6000)
        .collect();
    Ok(info)
}

fn request(url: &str, args: &[&str]) -> Result<String, String> {
    let output = hidden_command("curl")
        .args([
            "-q", // 不读用户 curlrc，不跟随重定向。
            "--silent",
            "--show-error",
            "--max-time",
            "8",
            "--proto",
            "=https",
            "--user-agent",
            "AgentSkillHub",
        ])
        .args(args)
        .arg(url)
        .output()
        .map_err(|e| format!("无法启动 curl：{e}"))?;
    if !output.status.success() {
        return Err(format!(
            "连接失败（curl {}）",
            output.status.code().unwrap_or(-1)
        ));
    }
    String::from_utf8(output.stdout).map_err(|_| "GitHub 响应不是有效的 UTF-8".into())
}

fn query_api(now: u64) -> Result<UpdateInfo, String> {
    let response = request(
        &format!("https://api.github.com/repos/{REPOSITORY}/releases/latest"),
        &[
            "--write-out",
            "\n%{http_code}",
            "-H",
            "Accept: application/vnd.github+json",
        ],
    )?;
    let (body, status) = response.rsplit_once('\n').ok_or("无法读取响应状态")?;
    if status != "200" {
        return Err(format!("HTTP {status}"));
    }
    let value: Value = serde_json::from_str(body).map_err(|_| "无法解析发布信息")?;
    parse_release(&value, now)
}

fn redirect(url: &str) -> Result<(String, String), String> {
    let response = request(
        url,
        &[
            "--head",
            "--output",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
            "--write-out",
            "%{http_code}\n%{redirect_url}",
        ],
    )?;
    parse_redirect_response(&response)
}

fn parse_redirect_response(response: &str) -> Result<(String, String), String> {
    response
        .split_once('\n')
        .map(|(code, target)| (code.trim_end_matches('\r').into(), target.into()))
        .ok_or_else(|| "无法读取发布页响应".into())
}

fn parse_latest_redirect(code: &str, target: &str, now: u64) -> Result<UpdateInfo, String> {
    if !matches!(code, "301" | "302" | "303" | "307" | "308") {
        return Err(format!("最新发布页未返回版本跳转（HTTP {code}）"));
    }
    release_info(target, now)
}

fn query_release_page(now: u64) -> Result<UpdateInfo, String> {
    let latest = format!("https://github.com/{REPOSITORY}/releases/latest");
    let (code, target) = redirect(&latest).or_else(|_| redirect(&latest))?;
    let mut info = parse_latest_redirect(&code, &target, now)?;
    if info.available {
        let tag = &info.latest_version;
        let asset = format!("https://github.com/{REPOSITORY}/releases/download/{tag}/AgentSkillHub-{tag}-portable.zip");
        // 只确认官方资产地址存在，不下载内容，也不跟随临时签名链接。
        if let Ok((code, target)) = redirect(&asset) {
            if code == "200"
                || (code == "302"
                    && target.starts_with("https://release-assets.githubusercontent.com/"))
            {
                info.download_url = asset;
            }
        }
    }
    Ok(info)
}

fn lookup_release(now: u64) -> Result<UpdateInfo, String> {
    query_api(now).or_else(|api| {
        query_release_page(now).map_err(|page| {
            format!("无法检查更新：GitHub API（{api}），官方发布页（{page}）。请通过浏览器查看官方发布。")
        })
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
        let info = lookup_release(now)?;
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

    #[test]
    fn latest_redirect_accepts_only_this_projects_stable_release() {
        let official = "https://github.com/fysh1010/AgentSkillHub/releases/tag/v999.0.0";
        let (code, target) = parse_redirect_response(&format!("302\r\n{official}")).unwrap();
        let info = parse_latest_redirect(&code, &target, 42).unwrap();
        assert_eq!(info.latest_version, "v999.0.0");
        assert_eq!(info.checked_at, 42);
        assert!(info.available);
        assert!(info.download_url.is_empty());
        assert!(parse_latest_redirect("200", official, 42).is_err());
        for target in [
            "https://github.com/other/project/releases/tag/v999.0.0",
            "https://github.com/fysh1010/AgentSkillHub/releases/tag/v999.0.0-beta",
            "https://github.com/fysh1010/AgentSkillHub/releases/tag/v999.0.0?download=1",
        ] {
            assert!(parse_latest_redirect("302", target, 42).is_err());
        }
    }

    #[test]
    #[ignore = "访问 GitHub 正式发布页；仅显式执行"]
    fn real_github_release_lookup() {
        let now = super::super::now_ts();
        let page = query_release_page(now).expect("官方发布页应可解析");
        let result = lookup_release(now).expect("应能检查正式更新");
        assert!(official_url(&result.release_url, false));
        println!(
            "官方发布页：{}；实际版本检查：{}",
            page.latest_version, result.latest_version
        );
    }
}
