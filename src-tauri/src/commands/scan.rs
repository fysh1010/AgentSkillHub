use crate::models::SkillInfo;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};

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

/// 目录大小缓存：path → (目录内容指纹, 大小)。
/// 每次勾选/部署后前端都会全量重扫，60+ 技能递归算大小是主要瓶颈；
/// 指纹未变就命中缓存，重扫基本零 I/O。
fn size_cache() -> &'static crate::commands::cache::MtimeCache<u64> {
    static CACHE: std::sync::OnceLock<crate::commands::cache::MtimeCache<u64>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

fn mtime_ms(md: &std::fs::Metadata) -> u64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 目录内容指纹：自身 + 两层内每个条目的修改时间（毫秒）之和，外加条目计数（顺序无关）。
/// 只用目录自身 mtime 是不够的——就地改写已存在的文件（改 SKILL.md、改 scripts/x.py）
/// 不碰父目录 mtime，缓存会永久陈旧；同一秒内的连续改动也要求毫秒精度。
/// 仍不覆盖「三层以上就地改文件」，这是遍历开销与失效精度的折中。
pub(crate) fn dir_fingerprint(path: &Path) -> u64 {
    fn walk(dir: &Path, depth: u32, acc: &mut u64) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let Ok(md) = fs::symlink_metadata(e.path()) else { continue };
            // 每个条目 +1：让「增删条目」也能改变指纹
            let name_hash = e.file_name().to_string_lossy().bytes().fold(0u64, |h,b| h.wrapping_mul(131).wrapping_add(b as u64));
            *acc = acc.wrapping_add(mtime_ms(&md)).wrapping_add(md.len()).wrapping_add(name_hash).wrapping_add(1);
            let name = e.file_name().to_string_lossy().to_string();
            if md.is_dir() && depth < 2 && !name.starts_with('.') && name != "node_modules" {
                walk(&e.path(), depth + 1, acc);
            }
        }
    }
    let mut acc = fs::metadata(path).ok().map(|md| mtime_ms(&md).wrapping_add(md.len())).unwrap_or(0);
    walk(path, 0, &mut acc);
    acc
}

/// 带缓存的目录大小（顶层入口）
fn dir_size_cached(path: &Path) -> u64 {
    let p = path.to_path_buf();
    crate::commands::cache::cached(size_cache(), p, dir_fingerprint(path), || dir_size(path, 0))
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

/// 扫描技能库根目录，支持任意层文件夹包装；遇到技能边界即停止向内扫描。
pub fn scan_skills(root: &str) -> Vec<SkillInfo> {
    let mut out = Vec::new();
    visit_skills(root, |path, name, category, key| {
        out.push(make_skill(path, name, category, key));
    });
    out
}

/// 枚举技能目录；汇总等调用方不必加载详情、大小和图标。
pub(super) fn visit_skills(root: &str, mut visit: impl FnMut(PathBuf, String, String, String)) {
    visit_skills_with_dirs(root, |path, name, category, key| visit(path, name, category, key), |_| {});
}

/// 共用遍历同时报告中间目录，供刷新检测观察深层文件夹中的增删。
pub(super) fn visit_skills_with_dirs(
    root: &str,
    mut visit: impl FnMut(PathBuf, String, String, String),
    mut directory: impl FnMut(&Path),
) {
    enum Step { Enter(PathBuf, String), Leave(PathBuf) }
    let mut stack = vec![Step::Enter(PathBuf::from(root), String::new())];
    let mut ancestors = HashSet::new();
    while let Some(step) = stack.pop() {
        let (path, key) = match step {
            Step::Enter(path, key) => (path, key),
            Step::Leave(real) => { ancestors.remove(&real); continue; }
        };
        let Ok(real) = fs::canonicalize(&path) else { continue };
        let real = super::deploy::normalize(&real);
        if !path.is_dir() || !ancestors.insert(real.clone()) { continue; }
        directory(&path);
        if !key.is_empty() && path.join("SKILL.md").is_file() {
            let name = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let category = key.rsplit_once('/').map(|(category, _)| category).unwrap_or("(根)").to_string();
            visit(path, name, category, key);
            ancestors.remove(&real);
            continue;
        }
        stack.push(Step::Leave(real));
        let Ok(entries) = fs::read_dir(&path) else { continue };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries.into_iter().rev() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(['.', '_']) || name == "node_modules" { continue; }
            let key = if key.is_empty() { name } else { format!("{key}/{name}") };
            if entry.path().is_dir() { stack.push(Step::Enter(entry.path(), key)); }
        }
    }
}

/// 仅枚举标识，不加载描述、图标和目录大小；市场共用现有扫描边界。
pub fn skill_names(root: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    visit_skills(root, |_, name, _, _| { names.insert(name.to_lowercase()); });
    names
}

pub fn find_skill_key(root: &str, name: &str) -> Result<Option<String>, String> {
    let mut keys = Vec::new();
    visit_skills(root, |_, candidate, _, key| {
        if candidate.eq_ignore_ascii_case(name) { keys.push(key); }
    });
    match keys.len() {
        0 => Ok(None),
        1 => Ok(keys.pop()),
        _ => Err(format!("技能库有多个同名技能「{name}」：{}，请先处理冲突", keys.join("、"))),
    }
}

/// 分类仅允许单个普通 Windows 目录名，根目录使用空字符串。
pub fn validate_category(category: &str) -> Result<(), String> {
    if category.is_empty() { return Ok(()); }
    let stem = category.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4 && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'));
    if category.starts_with(['.', '_']) || category.ends_with(['.', ' '])
        || category.chars().any(|c| c.is_control() || "<>:\"/\\|?*".contains(c)) || reserved
    {
        return Err("分类必须是单层普通文件夹名，不能使用隐藏、归档或系统保留名称".into());
    }
    Ok(())
}

fn safe_alias_key(key: &str) -> bool {
    !key.is_empty() && validate_category_path(key).is_ok()
}

/// 分类路径允许任意层级，每一段仍使用同样的路径边界检查。
pub fn validate_category_path(category: &str) -> Result<(), String> {
    if category.is_empty() { return Ok(()); }
    for part in category.split('/') {
        if part.is_empty() { return Err("分类路径不能包含空的目录名".into()); }
        validate_category(part)?;
    }
    Ok(())
}

/// 分类迁移映射只包含库内相对路径，可随技能库迁移；绝不按同名猜测。
pub fn skill_key_aliases(root: &str) -> HashMap<String, String> {
    #[derive(serde::Deserialize)]
    struct Aliases { version: u32, aliases: HashMap<String, String> }
    let file = Path::new(root).join(".agentskillhub-skill-aliases.json");
    if fs::metadata(&file).map(|m| m.len() > 1024 * 1024).unwrap_or(true) { return HashMap::new(); }
    let Ok(content) = fs::read(file) else { return HashMap::new() };
    if content.len() > 1024 * 1024 { return HashMap::new(); }
    let Ok(data) = serde_json::from_slice::<Aliases>(&content) else { return HashMap::new() };
    if data.version != 1 { return HashMap::new(); }
    data.aliases.into_iter().filter(|(old, new)| safe_alias_key(old) && safe_alias_key(new)).collect()
}

pub fn canonical_skill_key(root: &str, key: &str, aliases: &HashMap<String, String>) -> String {
    // 旧路径重新出现时以当前实体技能为准，映射不劫持新安装的同名技能。
    let mut current = key;
    let mut seen = HashSet::new();
    while seen.insert(current) {
        if safe_alias_key(current) && Path::new(root).join(current).join("SKILL.md").is_file() {
            let target = Path::new(root).join(current);
            if let (Ok(root), Ok(target)) = (fs::canonicalize(root), fs::canonicalize(target)) {
                if target.starts_with(root) { return current.to_string(); }
            }
            return key.to_string();
        }
        let Some(next) = aliases.get(current) else { break };
        current = next;
    }
    key.to_string()
}

pub fn normalize_skill_references(
    root: &str,
    agents: &mut HashMap<String, crate::models::AgentConfig>,
    meta: &mut HashMap<String, crate::models::SkillUserMeta>,
) {
    let aliases = skill_key_aliases(root);
    if aliases.is_empty() { return; }
    for config in agents.values_mut() {
        let mut seen = HashSet::new();
        config.enabled_skills = config.enabled_skills.iter()
            .map(|key| canonical_skill_key(root, key, &aliases))
            .filter(|key| seen.insert(key.clone())).collect();
    }
    let old = std::mem::take(meta);
    for (key, value) in old {
        let current = meta.entry(canonical_skill_key(root, &key, &aliases)).or_default();
        current.starred |= value.starred;
        for tag in value.tags {
            if !current.tags.contains(&tag) { current.tags.push(tag); }
        }
    }
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
pub(super) fn make_skill(path: PathBuf, dir_name: String, category: String, key: String) -> SkillInfo {
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
/// - 多层 key（"外层/开发/code"）→ root/外层/开发/code
pub fn resolve_skill_target(root: &str, key: &str) -> Option<PathBuf> {
    if !safe_alias_key(key) { return None; }
    let p = Path::new(root).join(key);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

const PREVIEW_LIMIT: usize = 200 * 1024;

fn read_preview(path: &Path) -> io::Result<String> {
    let mut bytes = Vec::new();
    fs::File::open(path)?.take((PREVIEW_LIMIT + 1) as u64).read_to_end(&mut bytes)?;
    let truncated = bytes.len() > PREVIEW_LIMIT;
    bytes.truncate(PREVIEW_LIMIT);
    let mut content = match String::from_utf8(bytes) {
        Ok(content) => content,
        Err(error) if truncated && error.utf8_error().error_len().is_none() => {
            let end = error.utf8_error().valid_up_to();
            let mut bytes = error.into_bytes();
            bytes.truncate(end);
            String::from_utf8(bytes).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
        }
        Err(error) => return Err(io::Error::new(io::ErrorKind::InvalidData, error)),
    };
    if truncated {
        content.push_str("…\n\n（内容过长，已截断）");
    }
    Ok(content)
}

/// 读取技能详情：SKILL.md 全文 + 文件清单（两层）+ 风险提示。
/// 供前端「查看全文」弹窗与安装风险提示使用。
pub fn read_skill_detail(root: &str, key: &str) -> Result<crate::models::SkillDetail, String> {
    use crate::models::{FileEntry, SkillDetail};
    let dir = resolve_skill_target(root, key).ok_or_else(|| format!("技能不存在: {key}"))?;

    // SKILL.md 全文（超长截断到 200KB，防内存/传输膨胀）
    let content = read_preview(&dir.join("SKILL.md")).unwrap_or_default();

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
    fn long_markdown_preview_truncates_at_utf8_boundary() {
        let dir = super::super::test_scratch("markdown_preview");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("SKILL.md");
        let prefix = "a".repeat(PREVIEW_LIMIT - 1);
        fs::write(&path, format!("{prefix}汉字")).unwrap();
        assert_eq!(read_preview(&path).unwrap(), format!("{prefix}…\n\n（内容过长，已截断）"));
    }

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

    /// 造一个混合结构的技能库：扁平技能 + 旧的「分类/技能」两层结构 + 各类应被忽略的目录。
    /// 每个用例传自己的 tag，避免并发跑测试时共用同一目录互相改写。
    fn fixture_root(tag: &str) -> PathBuf {
        let root = crate::commands::test_scratch(&format!("scan_lib_{tag}"));
        let write = |dir: &str, body: &str| {
            let d = root.join(dir);
            fs::create_dir_all(&d).unwrap();
            fs::write(d.join("SKILL.md"), body).unwrap();
        };
        write("gh-cli", "---\nname: gh-cli\ndescription: GitHub 命令\n---\n# 正文");
        write(
            "开发/code",
            "---\nname: code\ndisplay_name: 编码\ndescription: >\n  折叠\n  多行\n---\nx",
        );
        fs::create_dir_all(root.join("_archive/trash")).unwrap();
        fs::write(root.join("_archive/trash/SKILL.md"), "---\nname: a\n---\n").unwrap();
        fs::create_dir_all(root.join(".system")).unwrap();
        fs::write(root.join(".system/SKILL.md"), "---\nname: s\n---\n").unwrap();
        fs::create_dir_all(root.join("no-md")).unwrap();
        root
    }

    #[test]
    fn scan_flat_and_nested_layout() {
        let skills = scan_skills(&fixture_root("scan").to_string_lossy());
        let keys: Vec<&str> = skills.iter().map(|s| s.key.as_str()).collect();
        assert!(keys.contains(&"gh-cli"), "扁平结构应以目录名为 key");
        assert!(keys.contains(&"开发/code"), "两层结构应以 分类/名 为 key");
        assert!(
            !keys.contains(&"no-md"),
            "既无 SKILL.md 又无技能子目录的不算技能: {keys:?}"
        );
        // 归档目录与隐藏目录一律不参与扫描
        assert!(
            skills.iter().all(|s| !s.key.contains("_archive") && !s.key.contains(".system")),
            "不应包含归档/隐藏目录: {keys:?}"
        );
        let cli = skills.iter().find(|s| s.key == "gh-cli").unwrap();
        assert!(cli.healthy);
        assert_eq!(cli.category, "(根)");
        assert_eq!(cli.description, "GitHub 命令");
        let nested = skills.iter().find(|s| s.key == "开发/code").unwrap();
        assert_eq!(nested.category, "开发");
        assert_eq!(nested.name, "编码", "display_name 优先作为展示名");
        assert_eq!(nested.description, "折叠 多行", "块标量应折叠拼接");
    }

    #[test]
    fn scans_arbitrarily_wrapped_skills_without_entering_skill_examples() {
        let root = crate::commands::test_scratch("deep_skill_scan");
        let category = std::iter::repeat_n("w", 40).collect::<Vec<_>>().join("/");
        let key = format!("{category}/demo");
        let skill = root.join(&key);
        fs::create_dir_all(skill.join("examples/other")).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: demo\ndescription: nested\n---").unwrap();
        fs::write(skill.join("examples/other/SKILL.md"), "---\nname: other\n---").unwrap();
        for ignored in ["a/.git/bad", "a/_archive/bad", "a/node_modules/bad"] {
            fs::create_dir_all(root.join(ignored)).unwrap();
            fs::write(root.join(ignored).join("SKILL.md"), "---\nname: bad\n---").unwrap();
        }
        let text = root.to_string_lossy();
        let skills = scan_skills(&text);
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].key, key);
        assert_eq!(skills[0].category, category);
        assert_eq!(resolve_skill_target(&text, &key).unwrap(), skill);
        assert_eq!(find_skill_key(&text, "demo").unwrap(), Some(key));
        assert!(resolve_skill_target(&text, "a/../../outside").is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn deep_scan_breaks_junction_cycles_and_keeps_direct_skill_links() {
        let root = crate::commands::test_scratch("skill_scan_cycle");
        let skill = root.join("wrapper/deeper/demo");
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "---\nname: demo\n---").unwrap();
        super::super::deploy::create_junction(&root.join("wrapper/cycle"), &root).unwrap();
        super::super::deploy::create_junction(&root.join("direct-link"), &skill).unwrap();
        let skills = scan_skills(&root.to_string_lossy());
        assert_eq!(skills.len(), 2);
        assert!(skills.iter().any(|s| s.key == "wrapper/deeper/demo"));
        assert!(skills.iter().any(|s| s.key == "direct-link"));
        super::super::deploy::remove_junction(&root.join("wrapper/cycle")).unwrap();
        super::super::deploy::remove_junction(&root.join("direct-link")).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn category_index_stops_at_skill_boundary_and_reports_duplicate_names() {
        let root = fixture_root("category_index");
        let example = root.join("gh-cli/examples/example");
        fs::create_dir_all(&example).unwrap();
        fs::write(example.join("SKILL.md"), "---\nname: example\n---").unwrap();
        let root_str = root.to_string_lossy();
        let names = skill_names(&root_str);
        assert!(names.contains("code"));
        assert!(!names.contains("example"));
        assert_eq!(find_skill_key(&root_str, "CODE").unwrap().as_deref(), Some("开发/code"));
        fs::create_dir_all(root.join("other/code")).unwrap();
        fs::write(root.join("other/code/SKILL.md"), "---\nname: code\n---").unwrap();
        assert!(find_skill_key(&root_str, "code").unwrap_err().contains("多个同名"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn classification_aliases_preserve_metadata_and_reject_unsafe_targets() {
        let root = fixture_root("aliases");
        fs::write(root.join(".agentskillhub-skill-aliases.json"), r#"{"version":1,"aliases":{"code":"开发/code","unsafe":"../outside","loop":"loop","gh-cli":"开发/code"}}"#).unwrap();
        let root_str = root.to_string_lossy();
        let aliases = skill_key_aliases(&root_str);
        assert!(!aliases.contains_key("unsafe"));
        assert_eq!(canonical_skill_key(&root_str, "loop", &aliases), "loop");
        assert_eq!(canonical_skill_key(&root_str, "gh-cli", &aliases), "gh-cli");
        let mut agents = HashMap::from([("agent".into(), crate::models::AgentConfig { enabled_skills: vec!["code".into(), "开发/code".into(), "missing".into()] })]);
        let mut meta = HashMap::from([
            ("code".into(), crate::models::SkillUserMeta { starred: true, tags: vec!["编码".into()] }),
            ("开发/code".into(), crate::models::SkillUserMeta { starred: false, tags: vec!["开发".into()] }),
        ]);
        normalize_skill_references(&root_str, &mut agents, &mut meta);
        assert_eq!(agents["agent"].enabled_skills, ["开发/code", "missing"]);
        assert_eq!(meta.len(), 1);
        assert!(meta["开发/code"].starred);
        assert_eq!(meta["开发/code"].tags.len(), 2);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn category_names_reject_paths_and_windows_reserved_names() {
        for invalid in ["../outside", "a/b", "a\\b", "_archive", ".git", "CON", "con.txt", "LPT1", "a:", "a.", "a "] {
            assert!(validate_category(invalid).is_err(), "{invalid}");
        }
        for valid in ["", "01-开发工程", "音视频与交互"] { assert!(validate_category(valid).is_ok()); }
    }

    #[test]
    fn scan_missing_root_returns_empty() {
        assert!(scan_skills(r"Z:\绝对不存在的路径\skills").is_empty());
    }

    #[test]
    fn resolve_flat_and_nested() {
        let root = fixture_root("resolve").to_string_lossy().to_string();
        assert!(resolve_skill_target(&root, "gh-cli").is_some(), "扁平 key 应可解析");
        assert!(resolve_skill_target(&root, "开发/code").is_some(), "两层 key 兼容");
        assert!(resolve_skill_target(&root, "不存在分类/x").is_none());
    }

    #[test]
    fn fingerprint_detects_in_place_edit() {
        // 回归：就地改写已存在的文件不会更新父目录 mtime，旧实现据此起不了缓存
        let root = fixture_root("fingerprint");
        let dir = root.join("gh-cli");
        let before = dir_fingerprint(&dir);
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(dir.join("SKILL.md"), "---\nname: gh-cli\ndescription: 改了\n---\nx").unwrap();
        let edited = dir_fingerprint(&dir);
        assert_ne!(edited, before, "文件被就地改写后指纹必须变化");
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(dir.join("extra.txt"), b"x").unwrap();
        assert_ne!(dir_fingerprint(&dir), edited, "新增文件后指纹必须变化");
    }

    #[test]
    fn dir_size_cache_grows_with_content() {
        let dir = fixture_root("dirsize").join("gh-cli");
        assert!(dir_size_pub(&dir) > 0);
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(dir.join("blob.bin"), vec![b'7'; 4096]).unwrap();
        assert!(
            dir_size_pub(&dir) >= 4096,
            "缓存必须感知到新写入的文件，否则大小长期不更新"
        );
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
        let tmp = crate::commands::test_scratch("scan_icon");
        fs::create_dir_all(tmp.join("assets")).unwrap();
        fs::write(tmp.join("_icon.svg"), b"<svg/>").unwrap();
        fs::write(tmp.join("assets/logo.png"), b"\x89PNG").unwrap();
        // _icon.svg 优先级高于 assets/logo.png
        let got = find_icon_b64(&tmp);
        assert!(got.starts_with("data:image/svg+xml;base64,"), "got: {got}");
        // 没有任何图标时返回空
        let empty = crate::commands::test_scratch("scan_icon_empty");
        fs::create_dir_all(&empty).unwrap();
        fs::write(empty.join("readme.txt"), b"x").unwrap();
        assert_eq!(find_icon_b64(&empty), "");
        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_dir_all(&empty);
    }
}
