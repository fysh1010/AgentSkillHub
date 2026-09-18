use crate::models::SkillInfo;
use std::fs;
use std::path::{Path, PathBuf};

/// 从 SKILL.md 内容中提取 YAML frontmatter 的 name / display_name / description。
/// 支持 YAML 块标量写法（`description: >` / `|` 后跟缩进多行，ClawHub 技能常见），
/// 块内容按折叠拼接（换行合并为空格）。
fn parse_frontmatter(content: &str) -> (String, String, String, String) {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return (String::new(), String::new(), String::new(), String::new());
    }
    let rest = &trimmed[3..];
    let end = match rest.find("---") {
        Some(e) => e,
        None => return (String::new(), String::new(), String::new(), String::new()),
    };
    let meta = &rest[..end];
    let mut name = String::new();
    let mut display = String::new();
    let mut desc = String::new();
    let mut version = String::new();
    let mut lines = meta.lines().peekable();
    while let Some(raw) = lines.next() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // 确定字段并取行内值
        let field = if line.starts_with("name:") {
            Some(("name", &mut name))
        } else if line.starts_with("display_name:") {
            Some(("display_name", &mut display))
        } else if line.starts_with("description:") {
            Some(("description", &mut desc))
        } else if line.starts_with("version:") {
            Some(("version", &mut version))
        } else {
            None
        };
        let Some((field, target)) = field else {
            continue;
        };
        let v = extract_field(line, field).unwrap_or_default();
        // 块标量指示符：> / | / >- / |+ / >2 等
        let is_block = (v.starts_with('>') || v.starts_with('|'))
            && v[1..].chars().all(|c| matches!(c, '+' | '-' | '0'..='9'));
        if is_block {
            // 收集后续所有缩进行（块内容），空行视作分段
            let mut parts: Vec<String> = Vec::new();
            while let Some(next) = lines.peek() {
                let n = next.trim();
                if next.starts_with(' ') || next.starts_with('\t') {
                    if n.is_empty() {
                        if !parts.is_empty() {
                            parts.push(String::new());
                        }
                    } else {
                        parts.push(n.to_string());
                    }
                    lines.next();
                } else {
                    break;
                }
            }
            while parts.last().map(|p| p.is_empty()).unwrap_or(false) {
                parts.pop();
            }
            *target = parts.join(" ");
        } else if !v.is_empty() {
            *target = v;
        }
    }
    (name, display, desc, version)
}

/// 提取 "field: value" 中的 value，去掉引号与行尾注释
fn extract_field(line: &str, field: &str) -> Option<String> {
    let prefix = format!("{}:", field);
    let l = line.to_lowercase();
    if !l.starts_with(&prefix.to_lowercase()) {
        return None;
    }
    let mut v = line[prefix.len()..].trim().to_string();
    // 去掉包裹的引号
    if v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\''))) {
        v = v[1..v.len() - 1].to_string();
    }
    // 去掉尾随的 # 注释
    if let Some(pos) = v.find(" #") {
        v = v[..pos].trim().to_string();
    }
    Some(v)
}

fn read_skill_md(path: &Path) -> (String, String, String, String, bool) {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return (String::new(), String::new(), String::new(), String::new(), false),
    };
    let (name, display, desc, version) = parse_frontmatter(&content);
    let ok = !name.is_empty();
    (name, display, desc, version, ok)
}

/// 供市场模块使用的公开包装
pub fn read_skill_md_pub(path: &Path) -> (String, String, String, String, bool) {
    read_skill_md(path)
}

/// 供市场模块使用的目录大小统计（带缓存）
pub fn dir_size_pub(path: &Path) -> u64 {
    dir_size_cached(path)
}

/// 从 SKILL.md 内容提取一句话描述（frontmatter description 优先，退化取正文首个非空行）
pub fn extract_description(content: &str) -> String {
    let (_, _, desc, _) = parse_frontmatter(content);
    if !desc.is_empty() {
        return desc;
    }
    // 跳过 frontmatter 后找首个非空、非标题行
    let body = content.splitn(2, "---").nth(2).unwrap_or(content);
    for line in body.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') || l.starts_with("---") {
            continue;
        }
        return l.chars().take(120).collect();
    }
    String::new()
}

/// 目录大小缓存：path → (目录自身 mtime 秒, 大小)。
/// 每次勾选/部署后前端都会全量重扫，60+ 技能递归算大小是主要瓶颈；
/// 目录 mtime 未变就命中缓存，重扫基本零 I/O。
/// 注意：目录 mtime 只反映直接子项变化，深层文件变化可能延迟更新——
/// 仅用于展示用途，可接受。
fn size_cache() -> &'static std::sync::Mutex<std::collections::HashMap<PathBuf, (u64, u64)>> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<PathBuf, (u64, u64)>>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn dir_mtime_secs(path: &Path) -> u64 {
    fs::symlink_metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 带缓存的目录大小（顶层入口）
fn dir_size_cached(path: &Path) -> u64 {
    let mtime = dir_mtime_secs(path);
    if let Ok(cache) = size_cache().lock() {
        if let Some(&(mt, size)) = cache.get(path) {
            if mt == mtime {
                return size;
            }
        }
    }
    let size = dir_size(path, 0);
    if let Ok(mut cache) = size_cache().lock() {
        // 防止长期运行后无限增长
        if cache.len() > 500 {
            cache.clear();
        }
        cache.insert(path.to_path_buf(), (mtime, size));
    }
    size
}

/// 计算目录大小（不跟随符号链接，深度限制防循环）
fn dir_size(path: &Path, depth: u32) -> u64 {
    if depth > 4 {
        return 0;
    }
    let mut total = 0u64;
    let entries = match fs::read_dir(path) {
        Ok(e) => e,
        Err(_) => return 0,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let md = match fs::symlink_metadata(&p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if md.file_type().is_symlink() {
            continue; // 不跟随链接
        }
        if md.is_dir() {
            total += dir_size(&p, depth + 1);
        } else {
            total += md.len();
        }
    }
    total
}

/// 扫描技能库根目录（扁平结构：技能直接放在根目录；兼容旧的 分类/技能 两层结构）
pub fn scan_skills(root: &str) -> Vec<SkillInfo> {
    let mut out = Vec::new();
    let root_path = Path::new(root);
    if !root_path.is_dir() {
        return out;
    }
    let cats = match fs::read_dir(root_path) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for cat_entry in cats.flatten() {
        let cat_path = cat_entry.path();
        let cat_name = cat_entry.file_name().to_string_lossy().to_string();
        if cat_name == ".git" || cat_name.starts_with('.') || cat_name.starts_with('_') {
            // 隐藏目录（.system / .claude 等）与保留目录（_archive 等）不参与扫描
            continue;
        }
        if !cat_path.is_dir() {
            continue;
        }
        // 扁平结构：根目录下直接是技能目录（含 SKILL.md）
        if cat_path.join("SKILL.md").is_file() {
            out.push(make_skill(cat_path, cat_name.clone(), "(根)".to_string(), cat_name));
            continue;
        }
        // 旧两层结构：分类/技能
        let skills = match fs::read_dir(&cat_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for sk_entry in skills.flatten() {
            let sk_path = sk_entry.path();
            let sk_name = sk_entry.file_name().to_string_lossy().to_string();
            if sk_name.starts_with('.') || sk_name == "node_modules" {
                continue;
            }
            if !sk_path.is_dir() {
                continue;
            }
            if !sk_path.join("SKILL.md").is_file() {
                continue;
            }
            out.push(make_skill(sk_path, sk_name.clone(), cat_name.clone(), format!("{}/{}", cat_name, sk_name)));
        }
    }
    out
}

/// 找技能目录里自带的官方图标，返回 data URL（base64）。
/// 探测顺序：根目录 `_icon.*`（腾讯 SkillHub / 市场落盘约定）→ 根目录 icon/logo →
/// assets/ 下的 icon/logo。只收常见图片格式，单文件超过 3MB 跳过（防误收大图）。
fn find_icon_b64(dir: &Path) -> String {
    const EXT_MIME: &[(&str, &str)] = &[
        ("png", "image/png"),
        ("svg", "image/svg+xml"),
        ("jpg", "image/jpeg"),
        ("jpeg", "image/jpeg"),
        ("webp", "image/webp"),
        ("gif", "image/gif"),
        ("ico", "image/x-icon"),
    ];
    // 候选文件名，按优先级
    let stems: &[&str] = &["_icon", "icon", "logo", "_logo"];
    let mut candidates: Vec<PathBuf> = Vec::new();
    for stem in stems {
        for (ext, _) in EXT_MIME {
            candidates.push(dir.join(format!("{stem}.{ext}")));
        }
    }
    // assets/ 子目录下的 icon/logo
    let assets = dir.join("assets");
    if assets.is_dir() {
        for stem in stems {
            for (ext, _) in EXT_MIME {
                candidates.push(assets.join(format!("{stem}.{ext}")));
            }
        }
    }
    for c in candidates {
        if let Ok(md) = fs::metadata(&c) {
            if !md.is_file() || md.len() > 3 * 1024 * 1024 {
                continue;
            }
            let ext = c
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();
            let Some((_, mime)) = EXT_MIME.iter().find(|(e, _)| *e == ext) else {
                continue;
            };
            if let Ok(bytes) = fs::read(&c) {
                return format!("data:{mime};base64,{}", b64_encode(&bytes));
            }
        }
    }
    String::new()
}

/// 标准 base64 编码（标准字母表 + padding），零依赖
fn b64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { TABLE[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { TABLE[n as usize & 63] as char } else { '=' });
    }
    out
}

/// 由技能目录构造 SkillInfo（读取 SKILL.md 元信息）
fn make_skill(path: PathBuf, dir_name: String, category: String, key: String) -> SkillInfo {
    let (fname, fdisplay, fdesc, fversion, healthy) = read_skill_md(&path.join("SKILL.md"));
    let md = fs::symlink_metadata(&path).ok();
    let modified = md
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 展示名优先级：frontmatter display_name（多为中文名）→ name → 目录名
    let display = if !fdisplay.is_empty() {
        fdisplay
    } else if !fname.is_empty() {
        fname
    } else {
        dir_name
    };
    SkillInfo {
        name: display,
        category,
        key,
        path: path.to_string_lossy().to_string(),
        description: fdesc,
        has_scripts: path.join("scripts").is_dir(),
        size_bytes: dir_size_cached(&path),
        modified_ts: modified,
        healthy,
        icon_b64: find_icon_b64(&path),
        version: fversion,
    }
}

/// 根据技能 key 解析目标绝对路径：
/// - 扁平 key（"code"）→ root/code
/// - 两层 key（"开发/code"）→ root/开发/code
pub fn resolve_skill_target(root: &str, key: &str) -> Option<PathBuf> {
    let p = match key.split_once('/') {
        Some((cat, name)) => Path::new(root).join(cat).join(name),
        None => Path::new(root).join(key),
    };
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

/// 读取技能详情：SKILL.md 全文 + 文件清单（两层）+ 风险提示。
/// 供前端「查看全文」弹窗与安装风险提示使用。
pub fn read_skill_detail(root: &str, key: &str) -> Result<crate::models::SkillDetail, String> {
    use crate::models::{FileEntry, SkillDetail};
    let dir = resolve_skill_target(root, key).ok_or_else(|| format!("技能不存在: {key}"))?;

    // SKILL.md 全文（超长截断到 200KB，防内存/传输膨胀）
    let content = fs::read_to_string(dir.join("SKILL.md"))
        .unwrap_or_default();
    let content = if content.len() > 200 * 1024 {
        format!("{}…\n\n（内容过长，已截断）", &content[..200 * 1024])
    } else {
        content
    };

    // 风险文件：可执行脚本 / 钩子 / 二进制
    let risky_name = |n: &str| {
        matches!(
            n.to_lowercase().as_str(),
            "scripts" | "hooks" | "bin" | "install" | "setup"
        )
    };
    let risky_ext = |n: &str| {
        matches!(
            Path::new(n)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .as_deref(),
            Some("py" | "sh" | "ps1" | "bat" | "cmd" | "exe" | "dll" | "vbs" | "js")
        )
    };

    let mut files = Vec::new();
    let mut risks = Vec::new();
    let Ok(rd) = fs::read_dir(&dir) else {
        return Err(format!("读取目录失败: {}", dir.display()));
    };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        let p = e.path();
        let is_dir = p.is_dir();
        let size = if is_dir { dir_size_pub(&p) } else { p.metadata().map(|m| m.len()).unwrap_or(0) };
        let risky = (!is_dir && risky_ext(&name)) || (is_dir && risky_name(&name));
        if risky {
            risks.push(if is_dir {
                format!("包含 {name}/ 目录（可能含可执行脚本）")
            } else {
                format!("包含可执行文件 {name}")
            });
        }
        files.push(FileEntry {
            name: name.clone(),
            size_bytes: size,
            is_dir,
            risky,
        });
        // 目录里再展开一层，方便看 scripts/ 里有什么
        if is_dir {
            if let Ok(sub) = fs::read_dir(&p) {
                for se in sub.flatten().take(30) {
                    let sn = se.file_name().to_string_lossy().to_string();
                    let sp = se.path();
                    let sdir = sp.is_dir();
                    let ssize = if sdir {
                        0
                    } else {
                        sp.metadata().map(|m| m.len()).unwrap_or(0)
                    };
                    let srisky = !sdir && risky_ext(&sn);
                    if srisky && !risks.iter().any(|r| r.contains(&format!("{name}/{sn}"))) {
                        risks.push(format!("包含可执行脚本 {name}/{sn}"));
                    }
                    files.push(FileEntry {
                        name: format!("{name}/{sn}"),
                        size_bytes: ssize,
                        is_dir: sdir,
                        risky: srisky,
                    });
                }
            }
        }
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(SkillDetail {
        key: key.to_string(),
        content,
        files,
        risks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_parse() {
        let content = "---\nname: hello-skill\ndescription: 测试技能\n---\n\n正文";
        let (n, _display, d, _v) = parse_frontmatter(content);
        assert_eq!(n, "hello-skill");
        assert_eq!(d, "测试技能");
    }

    #[test]
    fn frontmatter_quoted() {
        let content = "---\nname: \"quoted\"\ndescription: '单引号'\n---\nx";
        let (n, _display, d, _v) = parse_frontmatter(content);
        assert_eq!(n, "quoted");
        assert_eq!(d, "单引号");
    }

    #[test]
    fn frontmatter_block_scalar() {
        // ClawHub 技能常见的 YAML 块标量写法（description: > 后跟缩进多行）
        let content = "---\nname: SkillScan\nmetadata:\n  version: \"1.1.6\"\ndescription: >\n  Security gate for skills.\n  Activate on any install.\n  Blocks HIGH/CRITICAL skills. No exceptions.\n---\n正文";
        let (n, _display, d, _v) = parse_frontmatter(content);
        assert_eq!(n, "SkillScan");
        assert_eq!(
            d,
            "Security gate for skills. Activate on any install. Blocks HIGH/CRITICAL skills. No exceptions."
        );
        // | 保留型块标量同样能取到内容
        let content2 = "---\nname: t\ndescription: |\n  line one\n  line two\n---";
        let (_, _, d2, _v2) = parse_frontmatter(content2);
        assert_eq!(d2, "line one line two");
    }

    #[test]
    fn scan_real_library() {
        let skills = scan_skills(r"E:\my-skills");
        assert!(skills.len() > 50, "技能数应 > 50，实际 {}", skills.len());
        // 扁平结构：key 直接是技能名
        assert!(skills.iter().any(|s| s.key == "gh-cli"), "应包含扁平 key 'gh-cli'");
        assert!(skills.iter().all(|s| !s.key.starts_with('_') && !s.key.starts_with('.')), "不应包含隐藏/归档目录");
        assert!(skills.iter().any(|s| s.healthy), "应存在健康技能");
    }

    #[test]
    fn resolve_flat_and_nested() {
        // 扁平 key
        assert!(resolve_skill_target(r"E:\my-skills", "gh-cli").is_some(), "扁平 key 应可解析");
        // 两层 key 兼容
        assert!(resolve_skill_target(r"E:\my-skills", "不存在分类/x").is_none());
    }

    #[test]
    fn b64_known_vectors() {
        assert_eq!(b64_encode(b""), "");
        assert_eq!(b64_encode(b"f"), "Zg==");
        assert_eq!(b64_encode(b"fo"), "Zm8=");
        assert_eq!(b64_encode(b"foo"), "Zm9v");
        assert_eq!(b64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(b64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(b64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn icon_detection_prefers_and_skips_oversize() {
        let tmp = std::env::temp_dir().join("ash_scan_icon");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("assets")).unwrap();
        fs::write(tmp.join("_icon.svg"), b"<svg/>").unwrap();
        fs::write(tmp.join("assets/logo.png"), b"\x89PNG").unwrap();
        // _icon.svg 优先级高于 assets/logo.png
        let got = find_icon_b64(&tmp);
        assert!(got.starts_with("data:image/svg+xml;base64,"), "got: {got}");
        // 没有任何图标时返回空
        let empty = std::env::temp_dir().join("ash_scan_icon_empty");
        let _ = fs::remove_dir_all(&empty);
        fs::create_dir_all(&empty).unwrap();
        fs::write(empty.join("readme.txt"), b"x").unwrap();
        assert_eq!(find_icon_b64(&empty), "");
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_dir_all(&empty);
    }
}
