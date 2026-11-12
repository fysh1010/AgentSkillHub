//! 技能市场：ClawHub 社区市场 + 任意 GitHub 技能仓库
//! HTTP 用系统 curl.exe，zip 解压用系统 tar.exe（Win10+ 自带 bsdtar），零额外依赖。

use crate::commands::scan;
use crate::models::*;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CLAWHUB_BASE: &str = "https://clawhub.ai";
const UA: &str = "AgentSkillHub/0.1";

/// 默认市场源（腾讯 SkillHub 排最前）
pub fn default_market_sources() -> Vec<MarketSource> {
    vec![
        MarketSource {
            id: "tencent-skillhub".into(),
            kind: "skillhub".into(),
            name: "腾讯 SkillHub".into(),
            repo: String::new(),
            subpath: String::new(),
        },
        MarketSource {
            id: "clawhub".into(),
            kind: "clawhub".into(),
            name: "ClawHub 社区市场".into(),
            repo: String::new(),
            subpath: String::new(),
        },
    ]
}

// ================= 基础工具 =================

/// 调用 curl 执行 GET，返回字节
fn http_get(url: &str) -> Result<Vec<u8>, String> {
    let out = crate::commands::hidden_command("curl")
        .args(["-sL", "--max-time", "60", "-A", UA, url])
        .output()
        .map_err(|e| format!("调用 curl 失败: {e}"))?;
    if !out.status.success() {
        return Err(format!("HTTP 请求失败 {}: {}", url, out.status));
    }
    Ok(out.stdout)
}

fn http_get_text(url: &str) -> Result<String, String> {
    let bytes = http_get(url)?;
    String::from_utf8(bytes).map_err(|e| format!("响应不是 UTF-8: {e}"))
}

fn tar_exe() -> &'static str {
    r"C:\Windows\System32\tar.exe"
}

/// 把 src 目录整体复制到 dst（dst 不能已存在）
fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| format!("创建目录失败: {e}"))?;
    let entries =
        std::fs::read_dir(src).map_err(|e| format!("读取目录失败 {}: {e}", src.display()))?;
    for e in entries.flatten() {
        let p = e.path();
        let name = e.file_name();
        let target = dst.join(&name);
        if p.is_dir() {
            copy_dir_all(&p, &target)?;
        } else {
            std::fs::copy(&p, &target).map_err(|e| format!("复制文件失败 {}: {e}", name.to_string_lossy()))?;
        }
    }
    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ================= ClawHub =================

/// ClawHub 搜索（q 为空时浏览默认 feed）
pub fn clawhub_search(query: &str, skills_root: &str) -> Result<Vec<MarketSkill>, String> {
    let q = urlencoding_lite(query);
    let url = format!("{CLAWHUB_BASE}/api/v1/search?q={q}&size=40");
    let body = http_get_text(&url)?;
    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("解析 ClawHub 响应失败: {e}"))?;
    let mut out = Vec::new();
    if let Some(items) = v.get("results").and_then(|r| r.as_array()) {
        for it in items {
            let reference = it
                .pointer("/install/reference")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            if reference.is_empty() {
                continue;
            }
            // reference 格式 "owner/slug"（全局技能无 owner）
            let (owner, slug) = match reference.split_once('/') {
                Some((o, s)) => (o.to_string(), s.to_string()),
                None => (String::new(), reference.clone()),
            };
            let display_name = it
                .get("displayName")
                .and_then(|x| x.as_str())
                .unwrap_or(&slug)
                .to_string();
            let description = it
                .pointer("/native/skill/summary")
                .or_else(|| it.get("summary"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let downloads = it.get("downloads").and_then(|x| x.as_u64()).unwrap_or(0);
            let updated_ms = it
                .pointer("/metrics/updatedAt")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            let installed = Path::new(skills_root).join(&slug).is_dir();
            // --- 富字段：图标 / 主题 / 精选 / 作者头像 / 安全扫描 / 版本 ---
            let icon = it
                .get("icon")
                .or_else(|| it.pointer("/native/skill/icon"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let topics: Vec<String> = it
                .pointer("/native/skill/topics")
                .or_else(|| it.pointer("/native/skill/categories"))
                .and_then(|x| x.as_array())
                .map(|a| {
                    a.iter()
                        .filter_map(|t| t.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let featured = it.get("featured").and_then(|x| x.as_bool()).unwrap_or(false);
            let owner_image = it
                .pointer("/native/owner/image")
                .or_else(|| it.pointer("/owner/image"))
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let security = it
                .pointer("/trust/upstreamScanners")
                .and_then(|x| x.as_object())
                .map(|m| {
                    let mut has_warn = false;
                    for (_, v) in m {
                        if let Some(st) = v.get("status").and_then(|x| x.as_str()) {
                            if st == "fail" || st == "error" {
                                return "fail".to_string();
                            }
                            if st == "warn" {
                                has_warn = true;
                            }
                        }
                    }
                    if has_warn {
                        "warn".to_string()
                    } else if m.is_empty() {
                        String::new()
                    } else {
                        "pass".to_string()
                    }
                })
                .unwrap_or_default();
            // 真实最新版本号（顶层 version 字段）；stats.versions 是版本总数，不能当版本号
            let version = it
                .get("version")
                .and_then(|x| x.as_str())
                .filter(|x| !x.is_empty())
                .map(|v| format!("v{v}"))
                .unwrap_or_default();
            // 官方分类与 star 数。
            // 注意：ClawHub 的 icon 字段恒为 null（该平台不提供技能图标），
            // 前端改用 publisher/owner 头像作为官方视觉。
            let category = it
                .pointer("/native/skill/categories")
                .and_then(|x| x.as_array())
                .and_then(|a| a.first())
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let stars = it
                .pointer("/native/skill/stats/stars")
                .and_then(|x| x.as_u64())
                .unwrap_or(0);
            out.push(MarketSkill {
                name: slug,
                display_name,
                description,
                downloads,
                updated_ts: updated_ms / 1000,
                installed,
                reference: reference.clone(),
                owner,
                icon,
                version,
                topics,
                featured,
                owner_image,
                source_kind: "clawhub".into(),
                security,
                icon_url: String::new(),
                category,
                stars,
            });
        }
    }
    Ok(out)
}

/// 把 ClawHub 列表接口的一个 items 数组转换成 MarketSkill
fn clawhub_list_items(
    v: &serde_json::Value,
    skills_root: &str,
) -> (Vec<MarketSkill>, Option<String>) {
    let mut out = Vec::new();
    for it in v.get("items").and_then(|x| x.as_array()).unwrap_or(&vec![]) {
        let slug = it.get("slug").and_then(|x| x.as_str()).unwrap_or("");
        if slug.is_empty() {
            continue;
        }
        let owner = it
            .get("ownerHandle")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string();
        let downloads = it
            .pointer("/stats/downloads")
            .and_then(|x| x.as_u64())
            .unwrap_or(0);
        let stars = it.pointer("/stats/stars").and_then(|x| x.as_u64()).unwrap_or(0);
        let topics: Vec<String> = it
            .get("topics")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|t| t.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        out.push(MarketSkill {
            name: slug.to_string(),
            display_name: it
                .get("displayName")
                .and_then(|x| x.as_str())
                .filter(|x| !x.is_empty())
                .unwrap_or(slug)
                .to_string(),
            description: it
                .get("summary")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string(),
            downloads,
            updated_ts: it
                .get("updatedAt")
                .and_then(|x| x.as_u64())
                .unwrap_or(0)
                / 1000,
            installed: Path::new(skills_root).join(slug).is_dir(),
            reference: if owner.is_empty() {
                slug.to_string()
            } else {
                format!("{owner}/{slug}")
            },
            owner,
            icon: String::new(),
            version: it
                .pointer("/tags/latest")
                .and_then(|x| x.as_str())
                .map(|v| format!("v{v}"))
                .unwrap_or_default(),
            topics: topics.clone(),
            featured: false,
            owner_image: String::new(),
            source_kind: "clawhub".into(),
            security: String::new(),
            icon_url: String::new(),
            category: topics.first().cloned().unwrap_or_default(),
            stars,
        });
    }
    let cursor = v
        .get("nextCursor")
        .and_then(|x| x.as_str())
        .map(String::from)
        .filter(|c| !c.is_empty());
    (out, cursor)
}

/// 拉取一个排序维度下的指定页（page 从 1 开始，单页约 25 条）。
/// cursor 只能顺序前进，取第 N 页需要从第 1 页逐步走过去；
/// 首页只需 1 个请求，配合前端懒加载把首次打开的耗时压到最低。
fn clawhub_fetch_page(
    sort: &str,
    page: u32,
    skills_root: &str,
) -> Result<Vec<MarketSkill>, String> {
    let mut cursor: Option<String> = None;
    for p in 1..=page {
        let url = match &cursor {
            Some(c) => format!(
                "{CLAWHUB_BASE}/api/v1/skills?sort={sort}&cursor={}",
                urlencoding_lite(c)
            ),
            None => format!("{CLAWHUB_BASE}/api/v1/skills?sort={sort}"),
        };
        let body = http_get_text(&url)?;
        let v: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| format!("解析 ClawHub 列表失败: {e}"))?;
        let (items, next) = clawhub_list_items(&v, skills_root);
        cursor = next;
        if p == page {
            return Ok(items);
        }
        if cursor.is_none() {
            return Ok(Vec::new());
        }
    }
    Ok(Vec::new())
}

/// ClawHub 默认推荐 feed：列表接口 `/api/v1/skills` 不带关键词也能查。
/// 按「下载量」和「star 数」两条线并行取第 page 页（首次只取第 1 页，约 50 条，
/// 用户滚到底部再翻页），合并去重后作为各分类的精选推荐。
/// 列表项没有官方分类字段，用首个 topic 充当分组维度。
pub fn clawhub_browse(skills_root: &str, page: u32) -> Result<Vec<MarketSkill>, String> {
    let page = page.max(1);
    let root_a = skills_root.to_string();
    let root_b = skills_root.to_string();
    let h_dl = std::thread::spawn(move || clawhub_fetch_page("downloads", page, &root_a));
    let h_star = std::thread::spawn(move || clawhub_fetch_page("stars", page, &root_b));
    // join() 返回 Result<Result<Vec,_>,_>：外层是线程 panic，内层是任务本身的错误
    let dl = h_dl.join().map_err(|_| "线程异常".to_string())??;
    let star = h_star.join().map_err(|_| "线程异常".to_string())??;

    let mut merged: Vec<MarketSkill> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for s in dl.into_iter().chain(star) {
        if seen.insert(s.name.clone()) {
            merged.push(s);
        }
    }
    // 合并后整体按下载量降序，前端再按分类分组
    merged.sort_by_key(|item| std::cmp::Reverse(item.downloads));
    Ok(merged)
}

/// ClawHub 下载安装：zip 解压后内容直接拷贝到 skills_root/{slug}
/// icon_url：市场卡片展示的图（ClawHub 无技能图标时为发布者头像），装完落盘
pub fn clawhub_install(
    slug: &str,
    owner: &str,
    skills_root: &str,
    icon_url: &str,
) -> Result<String, String> {
    if slug.is_empty() || slug.contains('/') || slug.contains("..") {
        return Err(format!("非法 slug: {slug}"));
    }
    let target = Path::new(skills_root).join(slug);
    if target.exists() {
        return Err(format!("技能库已存在同名技能「{slug}」，请先处理冲突"));
    }
    let mut url = format!("{CLAWHUB_BASE}/api/download?slug={}", urlencoding_lite(slug));
    if !owner.is_empty() {
        url.push_str(&format!("&ownerHandle={}", urlencoding_lite(owner)));
    }
    let bytes = http_get(&url)?;
    if bytes.len() < 4 || &bytes[..2] != b"PK" {
        return Err(format!("下载失败:{}", String::from_utf8_lossy(&bytes[..bytes.len().min(80)])).to_string());
    }
    let tmp = std::env::temp_dir().join(format!("ash_dl_{}", now_secs()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {e}"))?;
    let zip_path = tmp.join("skill.zip");
    std::fs::write(&zip_path, &bytes).map_err(|e| format!("写临时文件失败: {e}"))?;
    let result = (|| -> Result<(), String> {
        let out = crate::commands::hidden_command(tar_exe())
            .args([
                "-xf",
                &zip_path.to_string_lossy(),
                "-C",
                &tmp.to_string_lossy(),
            ])
            .output()
            .map_err(|e| format!("调用 tar 失败: {e}"))?;
        if !out.status.success() {
            return Err(format!("解压失败: {}", String::from_utf8_lossy(&out.stderr)));
        }
        // zip 内容平铺在 tmp 根（SKILL.md 等），整目录拷入技能库
        copy_dir_all(&tmp, &target)?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result?;
    // 官方图（或发布者头像）落盘：失败不阻塞安装
    if !icon_url.trim().is_empty() {
        let _ = ensure_skill_icon(slug, icon_url.trim(), skills_root);
    }
    Ok(slug.to_string())
}


// ================= 腾讯 SkillHub =================

/// 腾讯 SkillHub 官方 HTTP 接口。
/// 搜索直连（免去本地 Python CLI 的启动开销与控制台窗口）；
/// 安装仍走本地 CLI，以保留其签名校验链路。
const SKILLHUB_API: &str = "https://api.skillhub.cn";

const SKILLHUB_INSTALL_HINT: &str = "curl -fsSL https://skillhub-1388575217.cos.ap-guangzhou.myqcloud.com/install/install.sh | bash";

/// 本机腾讯 SkillHub CLI 路径（官方安装器装在 ~/.skillhub/skills_store_cli.py）
fn skillhub_cli_path() -> Result<PathBuf, String> {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .map_err(|_| "无法定位用户主目录".to_string())?;
    let p = Path::new(&home).join(".skillhub").join("skills_store_cli.py");
    if p.is_file() {
        Ok(p)
    } else {
        Err(format!(
            "未检测到腾讯 SkillHub CLI（{}）。\n请先安装：{SKILLHUB_INSTALL_HINT}",
            p.display()
        ))
    }
}

/// 调用 python 执行 skillhub CLI。
///
/// 必须强制 UTF-8：CLI 收尾会 `print("✓ Installed: …")`，而 GUI 进程继承的
/// 系统环境里 python 默认输出编码是 GBK，该字符无法编码 → UnicodeEncodeError
/// → 退出码非零。此时技能其实已经装好，属于「假失败」。
/// `PYTHONUTF8=1` 同时覆盖 stdin/stdout/stderr，`PYTHONIOENCODING` 兜底。
fn run_skillhub(args: &[&str]) -> Result<String, String> {
    let cli = skillhub_cli_path()?;
    let py = if cfg!(windows) { "python" } else { "python3" };
    let out = crate::commands::hidden_command(py)
        .arg(&cli)
        .args(args)
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONUTF8", "1")
        .output()
        .map_err(|e| format!("调用 skillhub CLI 失败: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "skillhub 命令失败: {}",
            skillhub_err_message(&out.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// 从 CLI 的 stderr 中提取真正有价值的错误信息。
///
/// CLI 把进度提示（`info:`、`Downloading:`）和错误写在同一流里，直接取第一行
/// 往往只拿到噪音。这里优先取 `Error:` 行；没有就退回最后一行（异常结论行）。
fn skillhub_err_message(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if let Some(e) = lines.iter().find(|l| l.starts_with("Error:")) {
        return (*e).to_string();
    }
    lines
        .last()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "未知错误".to_string())
}

/// 判断技能是否已安装。
///
/// CLI 把社区技能装到 `skills_root/@<owner>/<slug>`，同时兼容平铺位置。
/// 只查平铺路径会导致「装好了却仍显示可安装」，用户反复点击后撞上已存在错误。
fn skillhub_installed(skills_root: &str, owner: &str, slug: &str) -> bool {
    let root = Path::new(skills_root);
    if root.join(slug).is_dir() {
        return true;
    }
    if owner.is_empty() || slug.is_empty() {
        return false;
    }
    root.join(format!("@{owner}")).join(slug).is_dir()
}

/// 腾讯 SkillHub 搜索：直连官方搜索接口。
/// 相比调用本地 Python CLI（实测约 1.9s，且会弹出控制台窗口），直连约 0.5s，
/// 且能拿到 icon_url / category / downloads / stars / 官方中文描述等原始字段。
/// page 从 1 开始（服务端每页 60 条，前端用「加载更多」翻页）。
pub fn skillhub_search(query: &str, skills_root: &str, page: u32) -> Result<Vec<MarketSkill>, String> {
    let page = page.max(1);
    let mut url = format!("{SKILLHUB_API}/api/v1/search?limit=60&page={page}");
    let q = query.trim();
    if !q.is_empty() {
        url.push_str(&format!("&q={}", urlencoding_lite(q)));
    }
    let body = http_get_text(&url)?;
    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("解析 SkillHub 响应失败: {e}"))?;
    let mut out = Vec::new();
    if let Some(items) = v.get("results").and_then(|r| r.as_array()) {
        for it in items {
            let slug = it.get("slug").and_then(|x| x.as_str()).unwrap_or("");
            if slug.is_empty() {
                continue;
            }
            let display_name = it
                .get("displayName")
                .and_then(|x| x.as_str())
                .filter(|x| !x.is_empty())
                .unwrap_or(slug);
            // 官方双语描述：优先展示中文
            let description = it
                .get("description_zh")
                .and_then(|x| x.as_str())
                .filter(|x| !x.is_empty())
                .or_else(|| it.get("description").and_then(|x| x.as_str()))
                .or_else(|| it.get("summary").and_then(|x| x.as_str()))
                .unwrap_or("")
                .to_string();
            let version = it.get("version").and_then(|x| x.as_str()).unwrap_or("");
            let handle = it
                .pointer("/namespace/handle")
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let owner = if handle.is_empty() {
                it.get("owner_name").and_then(|x| x.as_str()).unwrap_or("")
            } else {
                handle
            };
            out.push(MarketSkill {
                name: slug.to_string(),
                display_name: display_name.to_string(),
                description,
                downloads: it.get("downloads").and_then(|x| x.as_u64()).unwrap_or(0),
                updated_ts: it
                    .get("updated_at")
                    .or_else(|| it.get("updatedAt"))
                    .and_then(|x| x.as_u64())
                    .unwrap_or(0)
                    / 1000,
                installed: skillhub_installed(skills_root, owner, slug),
                reference: slug.to_string(),
                owner: owner.to_string(),
                icon: String::new(),
                version: if version.is_empty() {
                    String::new()
                } else {
                    format!("v{version}")
                },
                topics: it
                    .get("tags")
                    .and_then(|x| x.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|t| t.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default(),
                featured: false,
                owner_image: String::new(),
                source_kind: "skillhub".into(),
                security: String::new(),
                icon_url: it
                    .get("icon_url")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                category: it
                    .get("category")
                    .and_then(|x| x.as_str())
                    .unwrap_or("")
                    .to_string(),
                stars: it.get("stars").and_then(|x| x.as_u64()).unwrap_or(0),
            });
        }
    }
    Ok(out)
}

/// 按公开 slug 读取官方详情、文件清单与 SKILL.md；文件预览失败时仍返回详情。
pub fn skillhub_detail(slug: &str) -> Result<crate::models::MarketSkillDetail, String> {
    use crate::models::{MarketSkillDetail, MarketSkillFile};

    let slug = slug.trim();
    if slug.is_empty() || slug.len() > 200 || slug.contains("..") || slug.contains('/') || slug.contains('\\') {
        return Err("无效的技能标识".into());
    }
    let base = format!("{SKILLHUB_API}/api/v1/skills/{}", urlencoding_lite(slug));
    let body = http_get_text(&base)?;
    let response: serde_json::Value = serde_json::from_str(&body)
        .map_err(|e| format!("解析 SkillHub 详情失败: {e}"))?;
    let skill = response.get("skill").ok_or_else(|| {
        response
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("SkillHub 未返回技能详情")
            .to_string()
    })?;
    let latest = response.get("latestVersion");
    let owner = response.get("owner");
    let string = |value: Option<&serde_json::Value>| {
        value.and_then(|v| v.as_str()).unwrap_or("").to_string()
    };
    let number = |value: Option<&serde_json::Value>| {
        value.and_then(|v| v.as_u64()).unwrap_or(0)
    };
    let labels = skill.get("labels");
    let mut detail = MarketSkillDetail {
        description: string(
            skill
                .get("summary_zh")
                .filter(|v| v.as_str().is_some_and(|s| !s.is_empty()))
                .or_else(|| skill.get("summary")),
        ),
        version: string(latest.and_then(|v| v.get("version"))),
        changelog: string(latest.and_then(|v| v.get("changelog"))),
        owner_name: string(owner.and_then(|v| v.get("displayName"))),
        publisher_source: string(skill.get("source")),
        category: string(skill.get("category")),
        downloads: number(skill.pointer("/stats/downloads")),
        stars: number(skill.pointer("/stats/stars")),
        installs: number(skill.pointer("/stats/installs")),
        versions: number(skill.pointer("/stats/versions")),
        created_ts: number(skill.get("createdAt")) / 1000,
        updated_ts: number(skill.get("updatedAt")) / 1000,
        requires_api_key: labels
            .and_then(|v| v.get("requires_api_key"))
            .and_then(|v| v.as_str())
            .map(|v| v == "true"),
        pricing_type: string(labels.and_then(|v| v.get("pricing_type"))),
        files: Vec::new(),
        skill_md: String::new(),
        preview_error: String::new(),
    };
    let version_query = if detail.version.is_empty() {
        String::new()
    } else {
        format!("version={}", urlencoding_lite(&detail.version))
    };
    let files_url = if version_query.is_empty() {
        format!("{base}/files")
    } else {
        format!("{base}/files?{version_query}")
    };
    match http_get_text(&files_url) {
        Ok(body) => {
            match serde_json::from_str::<serde_json::Value>(&body) {
                Ok(response) => {
                    if let Some(error) = response.get("error").and_then(|v| v.as_str()) {
                        detail.preview_error = error.to_string();
                        return Ok(detail);
                    }
                    detail.files = response
                        .get("files")
                        .and_then(|v| v.as_array())
                        .into_iter()
                        .flatten()
                        .filter_map(|file| Some(MarketSkillFile {
                            path: file.get("path")?.as_str()?.to_string(),
                            size_bytes: number(file.get("size")),
                        }))
                        .collect();
                    if detail.files.iter().any(|file| file.path.eq_ignore_ascii_case("SKILL.md")) {
                        let file_url = if version_query.is_empty() {
                            format!("{base}/file?path=SKILL.md")
                        } else {
                            format!("{base}/file?path=SKILL.md&{version_query}")
                        };
                        match http_get_text(&file_url) {
                            Ok(content) if !content.trim_start().starts_with("{\"error\"") => {
                                detail.skill_md = content;
                            }
                            Ok(_) => detail.preview_error = "官方文件预览暂时不可用".into(),
                            Err(error) => detail.preview_error = error,
                        }
                    } else {
                        detail.preview_error = "该技能没有公开 SKILL.md 预览".into();
                    }
                }
                Err(error) => detail.preview_error = format!("解析文件清单失败: {error}"),
            }
        }
        Err(error) => detail.preview_error = error,
    }
    Ok(detail)
}

/// 腾讯 SkillHub 安装：CLI install --namespace --dir
/// 注意：CLI 实际安装到 skills_root/@<namespace>/<slug> 的嵌套结构
pub fn skillhub_install(
    public_slug: &str,
    namespace: &str,
    skills_root: &str,
    icon_url: &str,
) -> Result<String, String> {
    let slug = public_slug.trim();
    if slug.is_empty() || slug.contains("..") || slug.contains(['/', '\\']) {
        return Err(format!("非法技能名: {public_slug}"));
    }
    let ns = namespace.trim();
    // 冲突检查：平铺路径与 @namespace 嵌套路径都不能已存在
    let flat = Path::new(skills_root).join(slug);
    let nested = if ns.is_empty() {
        flat.clone()
    } else {
        Path::new(skills_root).join(format!("@{ns}")).join(slug)
    };
    if flat.exists() || nested.exists() {
        return Err(format!("技能库已存在同名技能「{slug}」，请先处理冲突"));
    }
    let mut args = vec!["install", slug, "--dir", skills_root];
    if !ns.is_empty() {
        args.push("--namespace");
        args.push(ns);
    }
    // CLI 的失败有可能发生在「安装已完成、只是收尾输出报错」之后
    // （典型是 GBK 环境下打印 "✓ Installed" 抛 UnicodeEncodeError）。
    // 因此只要目标目录已落盘，就认定安装成功，不再把假失败抛给用户。
    match run_skillhub(&args) {
        Ok(text) => {
            if !nested.is_dir() && !flat.is_dir() {
                return Err(format!(
                    "skillhub 报告成功但未找到安装目录：{} 或 {}。\n{}",
                    nested.display(),
                    flat.display(),
                    text.lines().take(3).collect::<Vec<_>>().join("\n")
                ));
            }
        }
        Err(e) => {
            if !nested.is_dir() && !flat.is_dir() {
                return Err(e);
            }
        }
    }
    // CLI 会把技能装到 @namespace/<slug> 嵌套目录；技能库是扁平结构，
    // 嵌套位置不参与扫描（@ 开头）。装完直接搬回根目录，装好即可用。
    if nested.is_dir() && !flat.exists() {
        std::fs::rename(&nested, &flat).map_err(|e| {
            format!(
                "安装成功，但从 {} 归位到 {} 失败: {e}",
                nested.display(),
                flat.display()
            )
        })?;
        // 搬走后 @namespace 目录若已空，顺手清掉
        if let Some(ns_dir) = nested.parent() {
            if let Ok(mut rd) = std::fs::read_dir(ns_dir) {
                if rd.next().is_none() {
                    let _ = std::fs::remove_dir(ns_dir);
                }
            }
        }
    }
    // 官方图标落盘：失败不阻塞安装（图标只是锦上添花）
    if !icon_url.trim().is_empty() {
        let _ = ensure_skill_icon(slug, icon_url.trim(), skills_root);
    }
    Ok(slug.to_string())
}
// ================= GitHub 仓库源 =================

/// 列出 GitHub 仓库（子路径）下的技能目录
pub fn github_list(repo: &str, subpath: &str, skills_root: &str) -> Result<Vec<MarketSkill>, String> {
    let repo = repo.trim_matches('/');
    let sub = subpath.trim_matches('/');
    let url = if sub.is_empty() {
        format!("https://api.github.com/repos/{repo}/contents")
    } else {
        format!("https://api.github.com/repos/{repo}/contents/{sub}")
    };
    let body = http_get_text(&url)?;
    let v: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("解析 GitHub 响应失败: {e}"))?;
    let items = match v.as_array() {
        Some(a) => a.clone(),
        None => {
            let msg = v
                .get("message")
                .and_then(|x| x.as_str())
                .unwrap_or("未知错误");
            return Err(format!("GitHub API: {msg}（仓库或子路径可能不存在，或触发限流）"));
        }
    };
    let mut dirs: Vec<(String, String)> = Vec::new(); // (name, path)
    for it in &items {
        if it.get("type").and_then(|x| x.as_str()) != Some("dir") {
            continue;
        }
        let name = it.get("name").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let path = it.get("path").and_then(|x| x.as_str()).unwrap_or("").to_string();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        dirs.push((name, path));
    }
    dirs.sort();

    // 并发抓取每个技能的 SKILL.md 描述（8 线程）
    let repo_owned = repo.to_string();
    // 仓库 owner 的 GitHub 头像，作为该源的官方视觉
    let owner_avatar = repo_owned
        .split('/')
        .next()
        .map(|o| format!("https://github.com/{o}.png"))
        .unwrap_or_default();
    let mut out: Vec<MarketSkill> = Vec::new();
    for chunk in dirs.chunks(8) {
        let handles: Vec<_> = chunk
            .iter()
            .map(|(name, path)| {
                let repo = repo_owned.clone();
                let name = name.clone();
                let path = path.clone();
                std::thread::spawn(move || -> (String, String) {
                    let raw = format!(
                        "https://raw.githubusercontent.com/{repo}/HEAD/{path}/SKILL.md"
                    );
                    let desc = http_get_text(&raw)
                        .ok()
                        .map(|c| scan::extract_description(&c))
                        .unwrap_or_default();
                    (name, desc)
                })
            })
            .collect();
        for h in handles {
            let (name, desc) = h.join().map_err(|_| "线程异常".to_string())?;
            out.push(MarketSkill {
                installed: Path::new(skills_root).join(&name).is_dir(),
                reference: dirs
                    .iter()
                    .find(|(n, _)| n == &name)
                    .map(|(_, p)| p.clone())
                    .unwrap_or_else(|| name.clone()),
                name: name.clone(),
                display_name: name,
                description: desc,
                downloads: 0,
                updated_ts: 0,
                owner: repo_owned.clone(),
                icon: String::new(),
                version: String::new(),
                topics: Vec::new(),
                featured: false,
                owner_image: owner_avatar.clone(),
                source_kind: "github".into(),
                security: String::new(),
                icon_url: String::new(),
                category: "github".into(),
                stars: 0,
            });
        }
    }
    // 补 reference（github 用完整 path 安装）
    for (i, (name, path)) in dirs.iter().enumerate() {
        if let Some(s) = out.get_mut(i) {
            s.reference = path.clone();
            s.name = name.clone();
            s.display_name = name.clone();
            s.installed = Path::new(skills_root).join(name).is_dir();
        }
    }
    Ok(out)
}

/// GitHub 仓库安装：下载 tarball → 解压 → 拷贝对应技能目录
pub fn github_install(repo: &str, skill_path: &str, skills_root: &str) -> Result<String, String> {
    let repo = repo.trim_matches('/');
    let name = skill_path.rsplit('/').next().unwrap_or(skill_path);
    if name.is_empty() || name.contains("..") {
        return Err(format!("非法技能路径: {skill_path}"));
    }
    let target = Path::new(skills_root).join(name);
    if target.exists() {
        return Err(format!("技能库已存在同名技能「{name}」，请先处理冲突"));
    }
    let bytes = http_get(&format!("https://github.com/{repo}/archive/HEAD.tar.gz"))?;
    let tmp = std::env::temp_dir().join(format!("ash_dl_{}", now_secs()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).map_err(|e| format!("创建临时目录失败: {e}"))?;
    let tgz = tmp.join("repo.tar.gz");
    std::fs::write(&tgz, &bytes).map_err(|e| format!("写临时文件失败: {e}"))?;
    let result = (|| -> Result<(), String> {
        let ex = tmp.join("ex");
        std::fs::create_dir_all(&ex).map_err(|e| format!("创建解压目录失败: {e}"))?;
        let out = crate::commands::hidden_command(tar_exe())
            .args(["-xzf", &tgz.to_string_lossy(), "-C", &ex.to_string_lossy()])
            .output()
            .map_err(|e| format!("调用 tar 失败: {e}"))?;
        if !out.status.success() {
            return Err(format!("解压失败: {}", String::from_utf8_lossy(&out.stderr)));
        }
        // 找到解压后的唯一顶层目录
        let mut top: Option<PathBuf> = None;
        for e in std::fs::read_dir(&ex).map_err(|e| e.to_string())?.flatten() {
            if e.path().is_dir() {
                top = Some(e.path());
                break;
            }
        }
        let Some(top) = top else {
            return Err("tarball 结构异常：未找到顶层目录".into());
        };
        let src = top.join(skill_path);
        if !src.is_dir() {
            return Err(format!("仓库中不存在技能路径: {skill_path}"));
        }
        copy_dir_all(&src, &target)?;
        Ok(())
    })();
    let _ = std::fs::remove_dir_all(&tmp);
    result?;
    Ok(name.to_string())
}

/// 极简 URL 编码（字母数字和 -_.~ 之外全部转 %XX）
fn urlencoding_lite(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// 供翻译等其他模块复用
pub fn urlencoding_lite_pub(s: &str) -> String {
    urlencoding_lite(s)
}

// ================= 官方图标落盘 =================

/// 把市场里的官方图标下载保存为技能目录的 `_icon.<ext>`，
/// 这样本地扫描（find_icon_b64）就能直接识别，离线也有官方图标。
/// 已有任一可识别图标文件时直接跳过。
pub fn ensure_skill_icon(slug: &str, icon_url: &str, skills_root: &str) -> Result<String, String> {
    if slug.is_empty() || slug.contains("..") || slug.contains(['/', '\\']) {
        return Err(format!("非法技能名: {slug}"));
    }
    if icon_url.is_empty() || !icon_url.starts_with("http") {
        return Err("无效的图标地址".into());
    }
    let dir = Path::new(skills_root).join(slug);
    if !dir.is_dir() {
        return Err(format!("技能目录不存在: {}", dir.display()));
    }
    // 已有图标（含本地命名约定 _icon.*）就不重复下载
    for stem in ["_icon", "icon", "logo"] {
        for ext in ["png", "svg", "jpg", "jpeg", "webp", "gif", "ico"] {
            if dir.join(format!("{stem}.{ext}")).is_file() {
                return Ok("exists".into());
            }
        }
    }
    let bytes = http_get(icon_url)?;
    if bytes.is_empty() || bytes.len() > 3 * 1024 * 1024 {
        return Err("图标内容为空或超过 3MB".into());
    }
    // 按内容判别格式（比看 URL 扩展名可靠）
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]);
    let ext = if bytes.starts_with(b"\x89PNG") {
        "png"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "jpg"
    } else if head.trim_start().starts_with("<svg") || head.trim_start().starts_with("<?xml") {
        "svg"
    } else if bytes.len() > 12 && &bytes[8..12] == b"WEBP" {
        "webp"
    } else {
        "png"
    };
    let dest = dir.join(format!("_icon.{ext}"));
    std::fs::write(&dest, &bytes).map_err(|e| format!("写图标失败: {e}"))?;
    Ok(dest.to_string_lossy().to_string())
}

/// 收编：把 Agent 里的实体技能复制进技能库（含 .system 等系统技能，纯只读复制）
pub fn adopt(skills_dir: &str, dir_name: &str, skills_root: &str) -> Result<String, String> {
    if dir_name.is_empty() || dir_name.starts_with(['.', '_']) || dir_name.contains("..") || dir_name.contains('/') || dir_name.contains('\\') || dir_name.contains(':') {
        return Err(format!("非法目录名: {dir_name}"));
    }
    let src = Path::new(skills_dir).join(dir_name);
    if !src.is_dir() {
        return Err(format!("源目录不存在: {}", src.display()));
    }
    if !src.join("SKILL.md").is_file() { return Err("来源不是标准技能目录".into()); }
    let target = Path::new(skills_root).join(dir_name);
    if std::fs::symlink_metadata(&target).is_ok() {
        return Err(format!("技能库已存在同名技能「{dir_name}」"));
    }
    std::fs::create_dir_all(skills_root).map_err(|e| format!("无法创建技能库：{e}"))?;
    let source = std::fs::canonicalize(&src).map_err(|e| e.to_string())?;
    let root = std::fs::canonicalize(skills_root).map_err(|e| e.to_string())?;
    if root.starts_with(&source) { return Err("不能将技能导入其自身目录内部".into()); }
    // 独占创建目标，避免另一个工具在检查后创建同名技能时被覆盖。
    std::fs::create_dir(&target).map_err(|e| format!("无法创建导入目录（可能存在同名技能）：{e}"))?;
    fn copy_skill(src: &Path, dst: &Path, depth: usize, count: &mut usize) -> Result<(), String> {
        if depth > 24 { return Err("技能目录层级过深".into()); }
        for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let name = entry.file_name();
            if matches!(name.to_str(), Some(".git" | "node_modules")) { continue; }
            *count += 1;
            if *count > 12_000 { return Err("技能文件超过 12000 项，请先整理来源目录".into()); }
            let path = entry.path();
            let md = std::fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            if md.file_type().is_symlink() { return Err(format!("技能内部含外部链接，请先确认并整理：{}", path.display())); }
            let target = dst.join(name);
            if md.is_dir() {
                std::fs::create_dir(&target).map_err(|e| e.to_string())?;
                copy_skill(&path, &target, depth + 1, count)?;
            } else if md.is_file() {
                std::fs::copy(&path, &target).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    }
    if let Err(error) = copy_skill(&source, &target, 0, &mut 0) {
        // 只清理刚才独占创建、且从未写入链接的目录。
        let _ = std::fs::remove_dir_all(&target);
        return Err(error);
    }
    Ok(dir_name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_encoding() {
        assert_eq!(urlencoding_lite("abc-123"), "abc-123");
        assert_eq!(urlencoding_lite("a b/c"), "a%20b%2Fc");
    }

    #[test]
    fn adopt_copies_and_rejects_conflict() {
        let tmp = crate::commands::test_scratch("mkt_test");
        let lib = tmp.join("lib");
        let agent = tmp.join("agent");
        std::fs::create_dir_all(lib.join("existing")).unwrap();
        std::fs::create_dir_all(agent.join("myskill")).unwrap();
        std::fs::write(
            agent.join("myskill").join("SKILL.md"),
            "---\nname: myskill\ndescription: 测试\n---\n正文",
        )
        .unwrap();
        // 正常收编
        let r = adopt(agent.to_str().unwrap(), "myskill", lib.to_str().unwrap());
        assert!(r.is_ok());
        assert!(lib.join("myskill").join("SKILL.md").is_file());
        // 重名拒绝
        let r2 = adopt(agent.to_str().unwrap(), "myskill", lib.to_str().unwrap());
        assert!(r2.is_err());
        let r3 = adopt(agent.to_str().unwrap(), "existing", lib.to_str().unwrap());
        assert!(r3.is_err());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[cfg(windows)]
    #[test]
    fn adopt_refuses_nested_links_and_leaves_source_and_conflicts_intact() {
        let tmp = crate::commands::test_scratch("adopt_link_guard");
        let source = tmp.join("source/demo");
        let root = tmp.join("library");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::create_dir_all(tmp.join("private")).unwrap();
        std::fs::write(source.join("SKILL.md"), "---\nname: demo\ndescription: demo\n---\n").unwrap();
        std::fs::write(tmp.join("private/keep.txt"), "private").unwrap();
        super::super::deploy::create_junction(&source.join("linked"), &tmp.join("private")).unwrap();
        assert!(adopt(&tmp.join("source").to_string_lossy(), "demo", &root.to_string_lossy()).is_err());
        assert!(!root.join("demo").exists());
        assert!(source.join("SKILL.md").is_file());
        assert_eq!(std::fs::read_to_string(tmp.join("private/keep.txt")).unwrap(), "private");
        super::super::deploy::remove_junction(&source.join("linked")).unwrap();
        std::fs::remove_dir_all(tmp).unwrap();
    }

}

#[cfg(test)]
mod e2e_tests {
    use super::*;

    /// 真实网络端到端测试：从 ClawHub 安装一个技能到临时目录
    #[ignore]
    #[test]
    fn clawhub_real_install() {
        let tmp = crate::commands::test_scratch("mkt_e2e");
        std::fs::create_dir_all(&tmp).unwrap();
        let r = clawhub_install("screenshot", "ivangdavila", tmp.to_str().unwrap(), "");
        assert!(r.is_ok(), "安装失败: {:?}", r.err());
        assert!(tmp.join("screenshot").join("SKILL.md").is_file(), "SKILL.md 应存在");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
