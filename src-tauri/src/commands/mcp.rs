//! MCP 配置发现和按工具分配。扫描不启动服务；内置与插件来源始终只读。
use super::{agent_policy, AppState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};
use tauri::{Manager, State};

static IO: Mutex<()> = Mutex::new(());
static PLANS: Mutex<BTreeMap<String, Plan>> = Mutex::new(BTreeMap::new());
static SEQ: AtomicU64 = AtomicU64::new(0);

/// 仅查询公开 Registry，不执行市场返回的命令。
#[tauri::command]
pub async fn mcp_market_search(query: String, cursor: Option<String>) -> Result<Value, String> {
    if query.len() > 500 || cursor.as_ref().is_some_and(|s| s.len() > 2000) {
        return Err("搜索条件过长".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        fn escape(s: &str) -> String {
            s.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else { format!("%{b:02X}") }).collect()
        }
        let mut url = format!("https://registry.modelcontextprotocol.io/v0.1/servers?version=latest&limit=30&search={}", escape(query.trim()));
        if let Some(cursor) = cursor.filter(|s| !s.is_empty()) { url.push_str(&format!("&cursor={}", escape(&cursor))); }
        let out = super::hidden_command("curl")
            .args(["--fail", "--silent", "--show-error", "--location", "--proto", "=https", "--connect-timeout", "10", "--max-time", "25", "--max-filesize", "4194304", &url])
            .output().map_err(|e| format!("无法连接 MCP 市场：{e}"))?;
        if !out.status.success() { return Err("MCP 官方市场暂时无法访问，请检查网络后重试".into()); }
        let result: Value = serde_json::from_slice(&out.stdout).map_err(|_| "MCP 市场响应格式错误")?;
        if !result.get("servers").is_some_and(Value::is_array) { return Err("MCP 市场未返回服务列表".into()); }
        Ok(result)
    }).await.map_err(|e| e.to_string())?
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Server {
    pub name: String,
    pub config: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_timeout_sec: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct Binding {
    pub agent_id: String,
    pub path: String,
    /// json: mcpServers; vscode: servers; codex: mcp_servers (TOML);
    /// gemini: mcpServers/httpUrl; zcode: mcp.servers（嵌套，schema 严格）。
    pub format: String,
}
#[derive(Clone, Serialize, Deserialize, Default)]
struct Store {
    #[serde(default)]
    servers: BTreeMap<String, Server>,
    #[serde(default)]
    bindings: BTreeMap<String, Binding>,
    /// 只移除本应用曾部署且内容未被外部修改的配置。
    #[serde(default)]
    managed: BTreeMap<String, BTreeMap<String, Value>>,
    #[serde(default)]
    rollback: BTreeMap<String, Backup>,
    #[serde(default)]
    paused: BTreeMap<String, PausedSource>,
}
#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct PausedSource {
    binding: Binding,
    servers: BTreeMap<String, Value>,
    #[serde(default)]
    owned: Vec<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Backup {
    path: String,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
    managed: BTreeMap<String, Value>,
    #[serde(default)]
    paused_key: Option<String>,
    #[serde(default)]
    paused_before: Option<PausedSource>,
}
#[derive(Serialize)]
pub struct Source {
    pub id: String,
    pub agent_id: String,
    pub agent_name: String,
    pub path: String,
    pub format: String,
    pub kind: String,
    pub servers: Vec<Server>,
    pub error: String,
}
#[derive(Serialize)]
pub struct Target {
    pub agent_id: String,
    pub name: String,
    pub binding: Option<Binding>,
    pub installed: bool,
    pub writable: bool,
    pub managed: Vec<String>,
    pub can_restore: bool,
    /// MCP 只能在此工具自带界面内配置（如 Trae / 豆包），仅提供只读指引。
    pub app_managed: bool,
    pub guide: Option<super::mcp_catalog::Guide>,
    pub blocked_reason: String,
    pub reload_notice: String,
}
#[derive(Serialize)]
pub struct Inventory {
    pub servers: Vec<Server>,
    pub sources: Vec<Source>,
    pub targets: Vec<Target>,
    pub warnings: Vec<String>,
    pub guides: Vec<super::mcp_catalog::Guide>,
}
#[derive(Clone, Serialize)]
pub struct Action {
    pub name: String,
    pub kind: String,
}
#[derive(Serialize)]
pub struct Preview {
    pub ticket: String,
    pub path: String,
    pub actions: Vec<Action>,
}
struct Plan {
    ticket: String,
    agent_id: String,
    binding: Binding,
    before: Option<Vec<u8>>,
    after: Vec<u8>,
    managed: BTreeMap<String, Value>,
    next: BTreeMap<String, Value>,
    check_library: bool,
    paused_change: Option<(String, Option<PausedSource>, Option<PausedSource>)>,
}

fn store_path(state: &AppState) -> Result<PathBuf, String> {
    Ok(state
        .handle
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("mcp-library.json"))
}
fn load(state: &AppState) -> Result<Store, String> {
    let path = store_path(state)?;
    if !path.exists() {
        return Ok(Store::default());
    }
    serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|_| "MCP 管理库损坏，请先从备份恢复，未覆盖原文件".into())
}
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let parent = path.parent().ok_or("文件路径无父目录")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    // 两端统一为实际路径，避免 Windows 重定向目录/不同路径前缀触发跨卷重命名。
    let parent = fs::canonicalize(parent).map_err(|e| e.to_string())?;
    let destination = parent.join(path.file_name().ok_or("文件名为空")?);
    let tmp = parent.join(format!(
        ".agentskillhub-{}-{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    f.write_all(bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    drop(f);
    match replace_file(&tmp, &destination) {
        Ok(()) => Ok(()),
        // 部分企业加密过滤驱动禁止重命名/ReplaceFile；先落盘恢复副本再原位写入。
        Err(e) if cfg!(windows) && matches!(e.raw_os_error(), Some(17 | 6007)) => {
            write_recoverable(&destination, bytes)
        }
        Err(e) => Err(format!(
            "配置替换失败：{e}；目标：{}",
            destination.display()
        )),
    }
}
pub(super) fn write_recoverable(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let before = read_optional(path)?;
    let backup = path.with_file_name(format!(
        ".agentskillhub-recovery-{}.json",
        path.file_name().ok_or("文件名为空")?.to_string_lossy()
    ));
    let recovery =
        serde_json::to_vec(&json!({"path":path.to_string_lossy(),"before":before,"after":bytes}))
            .map_err(|e| e.to_string())?;
    let mut file =
        fs::File::create(&backup).map_err(|e| format!("无法保存恢复副本，未修改配置：{e}"))?;
    file.write_all(&recovery)
        .and_then(|_| file.sync_all())
        .map_err(|e| format!("恢复副本未完整落盘，未修改配置：{e}"))?;
    drop(file);
    if read_optional(path)? != before {
        return Err("保存前配置已被其他程序修改，请重新操作".into());
    }
    let write = |data: &[u8]| -> std::io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.write_all(data)?;
        file.sync_all()
    };
    if let Err(e) = write(bytes) {
        let restored = before
            .as_deref()
            .is_some_and(|original| write(original).is_ok());
        return Err(format!(
            "兼容保存失败：{e}；原内容{}；恢复副本：{}",
            if restored {
                "已恢复"
            } else {
                "请从副本恢复"
            },
            backup.display()
        ));
    }
    Ok(())
}
#[cfg(not(windows))]
fn replace_file(tmp: &Path, destination: &Path) -> std::io::Result<()> {
    fs::rename(tmp, destination)
}
#[cfg(windows)]
fn replace_file(tmp: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    // ReplaceFile 保留目标的 ACL/EFS 属性，避免加密目录中 MoveFileEx 替换失败。
    #[link(name = "kernel32")]
    extern "system" {
        fn ReplaceFileW(
            replaced: *const u16,
            replacement: *const u16,
            backup: *const u16,
            flags: u32,
            exclude: *mut std::ffi::c_void,
            reserved: *mut std::ffi::c_void,
        ) -> i32;
    }
    if !destination.exists() {
        return fs::rename(tmp, destination);
    }
    let to_wide = |p: &Path| {
        p.as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>()
    };
    let dest = to_wide(destination);
    let source = to_wide(tmp);
    let ok = unsafe {
        ReplaceFileW(
            dest.as_ptr(),
            source.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if ok == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn save(state: &AppState, data: &Store) -> Result<(), String> {
    atomic_write(
        &store_path(state)?,
        &serde_json::to_vec_pretty(data).map_err(|e| e.to_string())?,
    )
}
fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, String> {
    match fs::read(path) {
        Ok(bytes) if bytes.len() <= 8 * 1024 * 1024 => Ok(Some(bytes)),
        Ok(_) => Err("配置超过 8 MB，停止读取".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("无法读取 {}：{e}", path.display())),
    }
}
/// 将 JSONC 注释和尾逗号替换为空格，保持 UTF-8 字节偏移，供定点编辑使用。
fn clean_jsonc(text: &str) -> Result<String, String> {
    let mut bytes = text.as_bytes().to_vec();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i = string_end(&bytes, i)?;
            continue;
        }
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                bytes[i] = b' ';
                i += 1;
            }
        } else if bytes[i..].starts_with(b"/*") {
            bytes[i] = b' ';
            bytes[i + 1] = b' ';
            i += 2;
            while i + 1 < bytes.len() && &bytes[i..i + 2] != b"*/" {
                if bytes[i] != b'\n' && bytes[i] != b'\r' {
                    bytes[i] = b' ';
                }
                i += 1;
            }
            if i + 1 >= bytes.len() {
                return Err("JSONC 注释未闭合".into());
            }
            bytes[i] = b' ';
            bytes[i + 1] = b' ';
            i += 2;
        } else {
            i += 1;
        }
    }
    i = 0;
    while i < bytes.len() {
        if bytes[i] == b'"' {
            i = string_end(&bytes, i)?;
            continue;
        }
        if bytes[i] == b',' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < bytes.len() && b"}]".contains(&bytes[j]) {
                bytes[i] = b' ';
            }
        }
        i += 1;
    }
    String::from_utf8(bytes).map_err(|_| "JSONC 编码错误".into())
}
fn string_end(bytes: &[u8], start: usize) -> Result<usize, String> {
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            return Ok(i + 1);
        }
        i += 1;
    }
    Err("JSON 字符串未闭合".into())
}
/// 在 JSON 文本中定位 path（如 mcp.servers）指向的对象并整体替换为 map 的序列化结果。
/// path 逐级查找；任一层缺失时，在已存在的最深对象内插入剩余层级；节点外的字节原样保留。
fn replace_json_section(
    text: &str,
    path: &[&str],
    map: Map<String, Value>,
) -> Result<Vec<u8>, String> {
    let clean = clean_jsonc(text)?;
    let bytes = clean.as_bytes();
    let value = serde_json::to_string_pretty(&map).map_err(|e| e.to_string())?;
    let root_open = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .ok_or("JSON 根对象未找到")?;
    if bytes.get(root_open) != Some(&b'{') {
        return Err("JSON 根必须是对象".into());
    }
    let root_close = skip_container(bytes, root_open)? - 1;
    let result = match locate_node(bytes, root_open, root_close, path, 0)? {
        Node::Replace(start, end) => format!("{}{}{}", &text[..start], value, &text[end..]),
        Node::Insert { open, close, depth } => {
            // 自内向外构造 "key": {…} 嵌套；最内层直接挂 map，外层逐级包对象。
            let mut node = value;
            for (level, key) in path[depth..].iter().enumerate().rev() {
                let key_text = serde_json::to_string(key).map_err(|e| e.to_string())?;
                node = if level + depth + 1 == path.len() {
                    format!("{key_text}: {node}")
                } else {
                    format!("{key_text}: {{{node}}}")
                };
            }
            // 插在对象开头而不是闭括号前，避免与原文尾逗号、尾注释相互干扰。
            // 对象内是否还有其他内容按 clean 判断（注释已变空白，与解析器一致）。
            let has_entries = (open + 1..close).any(|k| !bytes[k].is_ascii_whitespace());
            let at = open + 1;
            format!(
                "{} {}{}{}",
                &text[..at],
                node,
                if has_entries { "," } else { "" },
                &text[at..]
            )
        }
    };
    // 输出再解析，避免任何注释/尾逗号组合导致无效配置。
    let _: Value = serde_json::from_str(&clean_jsonc(&result)?)
        .map_err(|_| String::from("保留注释编辑失败，未改动原配置"))?;
    Ok(result.into_bytes())
}
/// locate_node 的结果：替换目标对象，或在某层对象内插入缺失的 path 前缀。
enum Node {
    /// path 指向对象在 clean 文本中的字节区间。
    Replace(usize, usize),
    /// [open,close) 对象内缺少 path[depth..]，需在其闭括号前插入。
    Insert {
        open: usize,
        close: usize,
        depth: usize,
    },
}
/// 在 (open, close) 对象体内扫描键值，定位 path[depth..]；同层同名键重复即报错。
fn locate_node(
    bytes: &[u8],
    open: usize,
    close: usize,
    path: &[&str],
    depth: usize,
) -> Result<Node, String> {
    let mut i = open + 1;
    let mut found: Option<Node> = None;
    let mut matched = false;
    loop {
        while i < close && (bytes[i] == b',' || bytes[i].is_ascii_whitespace()) {
            i += 1;
        }
        if i >= close {
            break;
        }
        if bytes[i] != b'"' {
            return Err("对象内出现非字符串键，停止改写".into());
        }
        let key_end = string_end(bytes, i)?;
        let key = serde_json::from_slice::<String>(&bytes[i..key_end])
            .map_err(|_| String::from("JSON 键解析失败"))?;
        let mut j = key_end;
        while j < close && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if bytes.get(j) != Some(&b':') {
            return Err("对象键后缺少冒号，停止改写".into());
        }
        j += 1;
        while j < close && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        let value_end = skip_value(bytes, j, close)?;
        if key == path[depth] {
            if matched {
                return Err(format!("{} 节点重复，停止改写", path[depth]));
            }
            matched = true;
            if bytes.get(j) != Some(&b'{') {
                return Err(format!("{} 节点不是对象", path[depth]));
            }
            if depth + 1 == path.len() {
                found = Some(Node::Replace(j, value_end));
            } else {
                match locate_node(bytes, j, value_end - 1, path, depth + 1)? {
                    inner @ Node::Replace(..) => found = Some(inner),
                    // 前缀对象内缺更深层级：插入点落在该前缀对象的闭括号前。
                    Node::Insert { depth: deeper, .. } => {
                        return Ok(Node::Insert {
                            open: j,
                            close: value_end - 1,
                            depth: deeper,
                        });
                    }
                }
            }
        }
        i = value_end;
    }
    Ok(found.unwrap_or(Node::Insert { open, close, depth }))
}
/// 跳过位置 i 起的完整 JSON 值，返回其结束位置；不得超过 close。
fn skip_value(bytes: &[u8], mut i: usize, close: usize) -> Result<usize, String> {
    while i < close && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    match bytes.get(i) {
        Some(b'"') => {
            let end = string_end(bytes, i)?;
            if end > close {
                return Err("值越过对象边界".into());
            }
            Ok(end)
        }
        Some(b'{') | Some(b'[') => {
            let end = skip_container(bytes, i)?;
            if end > close {
                return Err("值越过对象边界".into());
            }
            Ok(end)
        }
        Some(c) if *c == b'-' || c.is_ascii_digit() || b"tfn".contains(c) => {
            while i < close && !b",}".contains(&bytes[i]) && !bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            Ok(i)
        }
        _ => Err("JSON 值格式错误，停止改写".into()),
    }
}
fn skip_container(bytes: &[u8], i: usize) -> Result<usize, String> {
    let mut depth = 0usize;
    let mut j = i;
    while j < bytes.len() {
        match bytes[j] {
            b'"' => {
                j = string_end(bytes, j)?;
                continue;
            }
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Ok(j + 1);
                }
            }
            _ => {}
        }
        j += 1;
    }
    Err("JSON 容器未闭合".into())
}
fn parse(bytes: Option<&[u8]>, format: &str) -> Result<Value, String> {
    let Some(bytes) = bytes else {
        return Ok(json!({}));
    };
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "配置不是 UTF-8，未改动原文件")?
        .trim_start_matches('\u{feff}');
    let value: Value = if is_toml(format) {
        toml_edit::de::from_str(text).map_err(|_| "TOML 配置格式错误，未改动原文件")?
    } else {
        serde_json::from_str(&clean_jsonc(text)?).map_err(|_| "JSON 配置格式错误，未改动原文件")?
    };
    if !value.is_object() {
        return Err("配置根节点必须是对象".into());
    }
    Ok(value)
}
fn is_toml(format: &str) -> bool {
    matches!(format, "codex" | "grok")
}
fn supported_format(format: &str) -> bool {
    [
        "json",
        "vscode",
        "codex",
        "grok",
        "gemini",
        "zcode",
        "opencode",
        "posit",
        "copilot",
        "cline",
        "kimi",
        "antigravity",
        "desktop",
        "crush",
        "pochi",
    ]
    .contains(&format)
}
fn node_path(format: &str) -> &'static [&'static str] {
    match format {
        "codex" | "grok" => &["mcp_servers"],
        "opencode" | "crush" | "pochi" => &["mcp"],
        "vscode" => &["servers"],
        // Zcode 把服务放在嵌套的 mcp.servers，且对未知字段零容忍。
        "zcode" => &["mcp", "servers"],
        _ => &["mcpServers"],
    }
}
fn has_node(doc: &Value, format: &str) -> bool {
    let mut node = doc;
    node_path(format).iter().all(|part| match node.get(part) {
        Some(next) => {
            node = next;
            true
        }
        None => false,
    })
}
fn entries(doc: &Value, format: &str) -> Result<Map<String, Value>, String> {
    let mut node = doc;
    for part in node_path(format) {
        match node.get(part) {
            None => return Ok(Map::new()),
            Some(next) if next.is_object() => node = next,
            Some(_) => return Err("MCP 服务节点不是对象，未改动原配置".into()),
        }
    }
    Ok(node.as_object().cloned().unwrap_or_default())
}
fn normalize(mut config: Value, format: &str) -> Value {
    if format == "cline" {
        if let Some(transport) = config.get("transport").and_then(Value::as_object).cloned() {
            let obj = config.as_object_mut().unwrap();
            obj.remove("transport");
            obj.extend(transport);
            if obj.get("type").and_then(Value::as_str) == Some("streamableHttp") {
                obj.insert("type".into(), json!("http"));
            }
        }
    }
    if let Some(obj) = config.as_object_mut() {
        if matches!(format, "opencode" | "posit") {
            if let Some(command) = obj.get("command").and_then(Value::as_array).cloned() {
                if let Some(first) = command.first() {
                    obj.insert("command".into(), first.clone());
                    obj.insert("args".into(), json!(&command[1..]));
                }
            }
            if let Some(env) = obj.remove("environment") {
                obj.insert("env".into(), env);
            }
            obj.remove("type");
        }
        if format == "antigravity" {
            if let Some(url) = obj.remove("serverUrl") {
                obj.insert("url".into(), url);
            }
        }
        if format == "kimi" {
            if let Some(t) = obj.remove("transport") {
                obj.insert("type".into(), t);
            }
        }
        if format == "copilot" && obj.get("type").and_then(Value::as_str) == Some("local") {
            obj.insert("type".into(), json!("stdio"));
        }
        if format == "codex" {
            if let Some(headers) = obj.remove("http_headers") {
                obj.insert("headers".into(), headers);
            }
        }
        if let Some(url) = obj.remove("httpUrl") {
            obj.insert("url".into(), url);
            obj.insert("type".into(), json!("http"));
        } else if format == "gemini" && obj.contains_key("url") {
            obj.entry("type").or_insert(json!("sse"));
        }
        if let Some(transport) = obj.remove("transportType") {
            obj.entry("type").or_insert(transport);
        }
    }
    config
}
fn validate(name: &str, config: &Value) -> Result<(), String> {
    if name.trim().is_empty() || name.len() > 160 || name.chars().any(char::is_control) {
        return Err("请输入有效的 MCP 名称（最多 160 字符）".into());
    }
    let obj = config.as_object().ok_or("服务配置必须是 JSON 对象")?;
    let command = obj
        .get("command")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty());
    let url = obj
        .get("url")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty());
    if command.is_some() == url.is_some() {
        return Err("服务必须且只能填写 command 或 url 中的一种".into());
    }
    if let Some(url) = url {
        if !url.starts_with("https://") && !url.starts_with("http://") && !url.contains("${") {
            return Err("远程 MCP 地址必须使用 http:// 或 https://".into());
        }
    }
    if let Some(args) = obj.get("args") {
        if !args
            .as_array()
            .is_some_and(|a| a.iter().all(Value::is_string))
        {
            return Err("args 必须为字符串数组".into());
        }
    }
    for key in ["env", "headers", "http_headers"] {
        if let Some(v) = obj.get(key) {
            if !v
                .as_object()
                .is_some_and(|m| m.values().all(Value::is_string))
            {
                return Err(format!("{key} 必须为字符串键值对象"));
            }
        }
    }
    Ok(())
}
fn native(server: &Server, format: &str) -> Result<Value, String> {
    validate(&server.name, &server.config)?;
    let raw = server.config.to_string();
    if raw.contains("${") || raw.contains("{{") {
        return Err(format!(
            "{} 依赖原工具的插件或输入变量，请先编辑为可移植配置",
            server.name
        ));
    }
    let mut obj = server.config.as_object().unwrap().clone();
    if obj.get("disabled").and_then(Value::as_bool) == Some(true)
        || obj.get("enabled").and_then(Value::as_bool) == Some(false)
    {
        return Err(format!(
            "{} 原配置已禁用，请在统一库中明确编辑启用后再分配",
            server.name
        ));
    }
    obj.remove("disabled");
    obj.remove("enabled");
    let transport = obj
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or(if obj.contains_key("command") {
            "stdio"
        } else {
            "http"
        })
        .to_string();
    if format == "desktop" && obj.contains_key("url") {
        return Err(
            "Claude Desktop 的本地配置只支持 STDIO；远程连接器请在 Claude Desktop 内添加".into(),
        );
    }
    if !["stdio", "http", "sse", "streamable-http"].contains(&transport.as_str()) {
        return Err(format!("{} 的传输类型暂不支持转换", server.name));
    }
    if format != "codex"
        && ["bearer_token_env_var", "env_http_headers", "env_vars"]
            .iter()
            .any(|key| obj.contains_key(*key))
    {
        return Err("此配置使用 Codex 专有环境变量字段，请先为目标工具编辑适用配置".into());
    }
    if matches!(format, "opencode" | "posit") {
        if let Some(command) = obj.remove("command") {
            let mut args = vec![command];
            if let Some(extra) = obj.remove("args").and_then(|v| v.as_array().cloned()) {
                args.extend(extra);
            }
            obj.insert("command".into(), Value::Array(args));
            if let Some(env) = obj.remove("env") {
                obj.insert("environment".into(), env);
            }
            obj.insert("type".into(), json!("local"));
        } else {
            obj.insert("type".into(), json!("remote"));
        }
        obj.insert("enabled".into(), json!(true));
        let allowed = [
            "type",
            "command",
            "environment",
            "enabled",
            "timeout",
            "url",
            "headers",
            "oauth",
        ];
        if let Some(key) = obj.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(format!(
                "{} 字段 {key} 尚不能无损转换为 {format}，请先编辑",
                server.name
            ));
        }
    } else if format == "cline" {
        obj.insert(
            "type".into(),
            json!(if transport == "http" || transport == "streamable-http" {
                "streamableHttp"
            } else {
                &transport
            }),
        );
        let mut outer = Map::new();
        let transport_keys = ["type", "command", "args", "env", "url", "headers", "cwd"];
        let extra: Vec<String> = obj
            .keys()
            .filter(|k| !transport_keys.contains(&k.as_str()))
            .cloned()
            .collect();
        for key in extra {
            outer.insert(key.clone(), obj.remove(&key).unwrap());
        }
        outer.insert("transport".into(), Value::Object(obj));
        return Ok(Value::Object(outer));
    } else if format == "antigravity" {
        obj.remove("type");
        if let Some(url) = obj.remove("url") {
            obj.insert("serverUrl".into(), url);
        }
        if transport == "sse" {
            obj.insert("type".into(), json!("sse"));
        }
    } else if format == "kimi" {
        obj.remove("type");
        if obj.contains_key("url") {
            obj.insert(
                "transport".into(),
                json!(if transport == "sse" { "sse" } else { "http" }),
            );
        }
    } else if format == "copilot" {
        obj.insert(
            "type".into(),
            json!(if obj.contains_key("command") {
                "local"
            } else if transport == "sse" {
                "sse"
            } else {
                "http"
            }),
        );
        obj.entry("tools").or_insert(json!(["*"]));
    } else if format == "grok" {
        if transport == "sse" {
            return Err("Grok Build 不支持 SSE，请使用 HTTP".into());
        }
        obj.remove("type");
    } else if format == "codex" {
        if transport == "sse" {
            return Err("Codex 不能直接使用 SSE 配置，请改用 Streamable HTTP 地址".into());
        }
        obj.remove("type");
        if let Some(headers) = obj.remove("headers") {
            obj.insert("http_headers".into(), headers);
        }
        // 不静默丢弃其他客户端的私有字段。
        let allowed = [
            "command",
            "args",
            "env",
            "env_vars",
            "cwd",
            "url",
            "http_headers",
            "env_http_headers",
            "bearer_token_env_var",
            "startup_timeout_sec",
            "startup_timeout_ms",
            "tool_timeout_sec",
            "enabled_tools",
            "disabled_tools",
            "required",
            "oauth",
            "scopes",
        ];
        if let Some(key) = obj.keys().find(|k| !allowed.contains(&k.as_str())) {
            return Err(format!(
                "{} 字段 {key} 无法无损转换为 Codex 配置，请先编辑",
                server.name
            ));
        }
    } else {
        if format == "zcode" {
            // Zcode 配置 schema 严格：未识别字段会让整个服务被静默丢弃，必须显式拦截。
            let allowed = [
                "type",
                "command",
                "args",
                "env",
                "cwd",
                "url",
                "headers",
                "timeoutMs",
            ];
            if let Some(key) = obj.keys().find(|k| !allowed.contains(&k.as_str())) {
                return Err(format!(
                    "{} 字段 {key} 不被 Zcode 支持（会被静默丢弃），请先编辑",
                    server.name
                ));
            }
            obj.insert(
                "type".into(),
                json!(if transport == "streamable-http" {
                    "http"
                } else {
                    transport.as_str()
                }),
            );
        } else if format == "gemini" {
            obj.remove("type");
            if transport != "sse" {
                if let Some(url) = obj.remove("url") {
                    obj.insert("httpUrl".into(), url);
                }
            }
        } else if format == "vscode" || obj.contains_key("url") {
            obj.insert(
                "type".into(),
                json!(if transport == "streamable-http" {
                    "http"
                } else {
                    &transport
                }),
            );
        }
    }
    Ok(Value::Object(obj))
}
fn encode(before: Option<&[u8]>, format: &str, map: Map<String, Value>) -> Result<Vec<u8>, String> {
    if is_toml(format) {
        let text = std::str::from_utf8(before.unwrap_or_default())
            .map_err(|_| "配置不是 UTF-8")?
            .trim_start_matches('\u{feff}');
        let mut doc = text
            .parse::<toml_edit::Document>()
            .map_err(|_| "TOML 格式错误")?;
        let new =
            toml_edit::ser::to_document(&json!({"mcp_servers":map})).map_err(|e| e.to_string())?;
        doc["mcp_servers"] = new
            .get("mcp_servers")
            .cloned()
            .unwrap_or(toml_edit::Item::None);
        Ok(doc.to_string().into_bytes())
    } else {
        let text = std::str::from_utf8(before.unwrap_or(b"{}"))
            .map_err(|_| "配置不是 UTF-8")?
            .trim_start_matches('\u{feff}');
        replace_json_section(text, node_path(format), map)
    }
}

fn binding(agent_id: &str) -> Option<Binding> {
    let (path, format) = super::mcp_catalog::resolve(agent_id)?;
    Some(Binding {
        agent_id: agent_id.into(),
        path: path.to_string_lossy().into_owned(),
        format,
    })
}

fn source(b: &Binding, agent_name: &str, kind: &str) -> Source {
    let result = (|| {
        let raw = read_optional(Path::new(&b.path))?;
        let doc = parse(raw.as_deref(), &b.format)?;
        if Path::new(&b.path)
            .file_name()
            .is_some_and(|n| n == ".app.json")
        {
            return Ok(doc
                .get("apps")
                .and_then(Value::as_object)
                .map(|apps| {
                    apps.keys()
                        .map(|name| Server {
                            check_timeout_sec: None,
                            name: name.clone(),
                            config: json!({"_hub_hosted":true}),
                        })
                        .collect()
                })
                .unwrap_or_default());
        }
        let map = if kind != "user" && !has_node(&doc, &b.format) {
            doc.as_object().cloned().unwrap_or_default()
        } else {
            entries(&doc, &b.format)?
        };
        Ok::<_, String>(
            map.into_iter()
                .filter(|(_, v)| v.is_object())
                .map(|(name, config)| Server {
                    check_timeout_sec: None,
                    name,
                    config: normalize(config, &b.format),
                })
                .collect(),
        )
    })();
    let (servers, error) = match result {
        Ok(servers) => (servers, String::new()),
        Err(e) => (Vec::new(), e),
    };
    Source {
        id: format!("{}:{}", b.agent_id, b.path),
        agent_id: b.agent_id.clone(),
        agent_name: agent_name.into(),
        path: b.path.clone(),
        format: b.format.clone(),
        kind: kind.into(),
        servers,
        error,
    }
}
/// 编辑器扩展的 MCP 配置与技能目录无关，须独立探测。
fn editor_sources(root: &Path) -> Vec<Source> {
    let mut out = Vec::new();
    for editor in ["Code", "Code - Insiders", "Cursor", "Windsurf"] {
        let user = root.join(editor).join("User");
        let mut profiles = vec![user.clone()];
        if let Ok(entries) = fs::read_dir(user.join("profiles")) {
            profiles.extend(
                entries
                    .flatten()
                    .take(64)
                    .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() && !t.is_symlink()))
                    .map(|e| e.path()),
            );
        }
        for profile in profiles {
            for (id, name, relative, format) in [
                ("copilot", "VS Code / Copilot", "mcp.json", "vscode"),
                (
                    "cline",
                    "Cline",
                    "globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json",
                    "json",
                ),
                (
                    "roo",
                    "Roo Code",
                    "globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json",
                    "json",
                ),
            ] {
                let path = profile.join(relative);
                if path.is_file() {
                    out.push(source(
                        &Binding {
                            agent_id: id.into(),
                            path: path.to_string_lossy().into_owned(),
                            format: format.into(),
                        },
                        name,
                        "user",
                    ));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod discovery_regressions {
    use super::*;
    #[test]
    fn client_specific_schemas_round_trip_without_changing_transport() {
        let local = Server {
            check_timeout_sec: None,
            name: "sample".into(),
            config: json!({"command":"npx","args":["-y","sample@1.0.0"],"env":{"TOKEN":"secret"}}),
        };
        let remote = Server {
            check_timeout_sec: None,
            name: "remote".into(),
            config: json!({"type":"http","url":"https://example.com/mcp","headers":{"X-Key":"secret"}}),
        };
        for format in [
            "json",
            "codex",
            "grok",
            "gemini",
            "zcode",
            "opencode",
            "posit",
            "copilot",
            "cline",
            "kimi",
            "antigravity",
            "crush",
            "pochi",
        ] {
            for server in [&local, &remote] {
                let encoded = native(server, format).unwrap();
                let decoded = normalize(encoded, format);
                assert_eq!(
                    decoded.get("command"),
                    server.config.get("command"),
                    "{format}"
                );
                assert_eq!(decoded.get("args"), server.config.get("args"), "{format}");
                assert_eq!(decoded.get("url"), server.config.get("url"), "{format}");
                assert_eq!(decoded.get("env"), server.config.get("env"), "{format}");
                assert_eq!(
                    decoded.get("headers"),
                    server.config.get("headers"),
                    "{format}"
                );
            }
        }
        assert!(native(&remote, "desktop").unwrap_err().contains("STDIO"));
        assert_eq!(native(&local, "copilot").unwrap()["type"], "local");
        assert_eq!(native(&local, "copilot").unwrap()["tools"], json!(["*"]));
        assert!(native(&local, "opencode").unwrap()["command"].is_array());
        assert_eq!(
            native(&remote, "antigravity").unwrap()["serverUrl"],
            "https://example.com/mcp"
        );
        assert!(native(&remote, "cline").unwrap()["transport"].is_object());
        let filtered = Server {
            check_timeout_sec: None,
            name: "cline".into(),
            config: json!({"command":"never-run","autoApprove":["read"]}),
        };
        assert_eq!(
            native(&filtered, "cline").unwrap()["autoApprove"],
            json!(["read"])
        );
        assert!(native(&filtered, "cline").unwrap()["transport"]
            .get("autoApprove")
            .is_none());
        let private = Server {
            check_timeout_sec: None,
            name: "private".into(),
            config: json!({"url":"https://example.com/mcp","bearer_token_env_var":"TOKEN"}),
        };
        for format in ["opencode", "posit", "cline", "antigravity", "copilot"] {
            assert!(native(&private, format).is_err());
        }
    }
    #[test]
    fn compatible_save_keeps_a_durable_original_copy() {
        let root = std::env::temp_dir().join(format!(
            "mcp-save-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        let original = b"{\"theme\":\"dark\"}";
        fs::write(&path, original).unwrap();
        write_recoverable(&path, b"{\"theme\":\"light\"}").unwrap();
        let recovery: Value = serde_json::from_slice(
            &fs::read(root.join(".agentskillhub-recovery-settings.json.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            serde_json::from_value::<Vec<u8>>(recovery["before"].clone()).unwrap(),
            original
        );
        assert_eq!(fs::read(&path).unwrap(), b"{\"theme\":\"light\"}");
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn editor_profiles_are_discovered_without_skills() {
        let root = std::env::temp_dir().join(format!(
            "mcp-discovery-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let profile = root.join("Code/User/profiles/work");
        fs::create_dir_all(&profile).unwrap();
        fs::write(
            profile.join("mcp.json"),
            br#"{"servers":{"remote":{"type":"http","url":"https://example.com/mcp"}}}"#,
        )
        .unwrap();
        let extension = root.join("Cursor/User/globalStorage/saoudrizwan.claude-dev/settings");
        fs::create_dir_all(&extension).unwrap();
        fs::write(
            extension.join("cline_mcp_settings.json"),
            br#"{"mcpServers":{"local":{"command":"never-executed"}}}"#,
        )
        .unwrap();
        let sources = editor_sources(&root);
        assert_eq!(sources.len(), 2);
        assert!(sources
            .iter()
            .all(|s| s.error.is_empty() && s.servers.len() == 1));
        assert!(sources
            .iter()
            .any(|s| s.agent_id == "copilot" && s.format == "vscode"));
        assert!(sources.iter().any(|s| s.agent_id == "cline"));
        fs::remove_dir_all(root).unwrap();
    }
}

#[tauri::command]
pub fn mcp_inventory(state: State<'_, AppState>) -> Result<Inventory, String> {
    let _guard = IO.lock().unwrap();
    inventory(&state)
}
fn inventory(state: &AppState) -> Result<Inventory, String> {
    let data = load(state)?;
    let cfg = state.config.lock().unwrap();
    let inventory = state.inventory.lock().unwrap();
    let mut agents: BTreeMap<String, (String, bool)> = super::known_agents::KNOWN_AGENTS
        .iter()
        .filter(|a| !["agents", "config-agents"].contains(&a.id))
        .map(|a| (a.id.into(), (a.name.into(), false)))
        .collect();
    if let Some(inv) = inventory.as_ref() {
        for a in &inv.agents {
            if !a.shared {
                agents.insert(a.id.clone(), (a.name.clone(), a.installed));
            }
        }
    }
    for a in &cfg.agents {
        agents
            .entry(a.id.clone())
            .or_insert((a.name.clone(), false));
    }
    if binding("claude-desktop").is_some_and(|b| Path::new(&b.path).is_file()) {
        agents
            .entry("claude-desktop".into())
            .or_insert(("Claude Desktop".into(), false));
    }
    let mut sources = Vec::new();
    let mut targets = Vec::new();
    for (id, (name, installed)) in agents {
        let b = data.bindings.get(&id).cloned().or_else(|| binding(&id));
        if let Some(b) = &b {
            if Path::new(&b.path).exists() {
                sources.push(source(b, &name, "user"));
            }
        }
        targets.push(Target {
            agent_id: id.clone(),
            name,
            installed,
            writable: b.is_some()
                && agent_policy::require_agent(&cfg, inventory.as_ref(), &id).is_ok(),
            binding: b,
            managed: data
                .managed
                .get(&id)
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default(),
            can_restore: data.rollback.contains_key(&id),
            // Trae / 豆包等只在应用内提供 MCP 配置界面，不猜测可写文件。
            app_managed: !data.bindings.contains_key(&id)
                && super::mcp_catalog::guide(&id).is_some_and(|g| g.format.is_empty()),
            guide: super::mcp_catalog::guide(&id).cloned(),
            reload_notice: super::mcp_catalog::reload_notice(&id).into(),
            blocked_reason: agent_policy::require_agent(&cfg, inventory.as_ref(), &id)
                .err()
                .unwrap_or_default(),
        });
    }
    // 对所有发现的 Agent 补查技能目录旁的常见配置，只读发现不代表格式写入已适配。
    if let Some(inv) = inventory.as_ref() {
        for agent in &inv.agents {
            if agent.shared {
                continue;
            }
            let Some(dir) = Path::new(&agent.skills_dir).parent() else {
                continue;
            };
            for file in ["mcp.json", ".mcp.json", "mcp_config.json", "settings.json"] {
                let p = dir.join(file);
                if !p.is_file()
                    || sources
                        .iter()
                        .any(|s| super::known_agents::eq_path(Path::new(&s.path), &p))
                {
                    continue;
                }
                let b = Binding {
                    agent_id: agent.id.clone(),
                    path: p.to_string_lossy().into_owned(),
                    format: "json".into(),
                };
                let s = source(&b, &agent.name, "user");
                if !s.servers.is_empty() || file.contains("mcp") {
                    sources.push(s);
                }
            }
        }
    }
    if let Some(root) = super::env_dir("APPDATA") {
        for s in editor_sources(&root) {
            if !sources.iter().any(|old| {
                old.agent_id == s.agent_id
                    && super::known_agents::eq_path(Path::new(&old.path), Path::new(&s.path))
            }) {
                sources.push(s);
            }
        }
    }
    for (key, paused) in &data.paused {
        if paused.servers.is_empty() {
            continue;
        }
        sources.push(Source {
            id: format!("{key}:paused"),
            agent_id: paused.binding.agent_id.clone(),
            agent_name: targets
                .iter()
                .find(|t| t.agent_id == paused.binding.agent_id)
                .map(|t| t.name.clone())
                .unwrap_or_else(|| paused.binding.agent_id.clone()),
            path: paused.binding.path.clone(),
            format: paused.binding.format.clone(),
            kind: "paused".into(),
            servers: paused
                .servers
                .iter()
                .map(|(name, config)| Server {
                    check_timeout_sec: None,
                    name: name.clone(),
                    config: normalize(config.clone(), &paused.binding.format),
                })
                .collect(),
            error: String::new(),
        });
    }
    targets.sort_by_key(|t| (!t.installed, t.name.clone()));
    let warnings = vec!["仅发现明确的用户配置文件，不深搜插件与缓存。配置写入不代表运行中的 Agent 已重载或完成认证。".into()];
    Ok(Inventory {
        servers: data.servers.into_values().collect(),
        sources,
        targets,
        warnings,
        guides: super::mcp_catalog::guides().clone(),
    })
}

#[tauri::command]
pub fn mcp_save_server(state: State<'_, AppState>, server: Server) -> Result<(), String> {
    let _guard = IO.lock().unwrap();
    validate(&server.name, &server.config)?;
    if server.check_timeout_sec.is_some_and(|s| s == 0 || s > 300) {
        return Err("启动检测超时应为 1–300 秒".into());
    }
    let mut data = load(&state)?;
    data.servers.insert(server.name.clone(), server);
    save(&state, &data)
}
#[tauri::command]
pub fn mcp_remove_server(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let _guard = IO.lock().unwrap();
    let mut data = load(&state)?;
    if data.managed.values().any(|m| m.contains_key(&name)) {
        return Err("该 MCP 仍有工具分配，请先取消分配并部署".into());
    }
    data.servers.remove(&name);
    save(&state, &data)
}
#[tauri::command]
pub fn mcp_save_binding(state: State<'_, AppState>, binding: Binding) -> Result<(), String> {
    let _guard = IO.lock().unwrap();
    let path = Path::new(&binding.path);
    if !path.is_absolute() || !supported_format(&binding.format) {
        return Err("请选择绝对配置路径及支持的格式".into());
    }
    if !path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        if is_toml(&binding.format) {
            e == "toml"
        } else {
            ["json", "jsonc"].contains(&e)
        }
    }) {
        return Err("配置文件扩展名应与格式一致".into());
    }
    if path.starts_with(
        state
            .handle
            .path()
            .app_config_dir()
            .map_err(|e| e.to_string())?,
    ) {
        return Err("不能将本应用的数据文件作为 Agent 配置".into());
    }
    let segments: Vec<_> = path
        .components()
        .map(|s| s.as_os_str().to_string_lossy().to_lowercase())
        .collect();
    if segments.iter().any(|s| {
        [
            "cache",
            "plugins",
            "connectors",
            "builtin",
            "marketplaces",
            "connectors-marketplace",
        ]
        .contains(&s.as_str())
    }) {
        return Err("内置、插件与市场目录只读，不能作为部署目标".into());
    }
    let mut data = load(&state)?;
    if data
        .managed
        .get(&binding.agent_id)
        .is_some_and(|m| !m.is_empty())
    {
        return Err("请先取消此工具的全部 MCP 分配，再更改配置位置".into());
    }
    let cfg = state.config.lock().unwrap();
    let inv = state.inventory.lock().unwrap();
    agent_policy::require_agent(&cfg, inv.as_ref(), &binding.agent_id)?;
    let raw = read_optional(path)?;
    let doc = parse(raw.as_deref(), &binding.format)?;
    entries(&doc, &binding.format)?;
    data.bindings.insert(binding.agent_id.clone(), binding);
    save(&state, &data)
}

/// 把一个本机 MCP 配置文件里的全部服务批量收编进统一库。
/// 同名服务自动加来源后缀；依赖原工具变量的不可移植配置跳过并计数。
#[tauri::command]
pub fn mcp_adopt_source(
    state: State<'_, AppState>,
    path: String,
    format: String,
    agent_id: String,
) -> Result<String, String> {
    let _guard = IO.lock().unwrap();
    if !["json", "vscode", "codex", "gemini", "zcode"].contains(&format.as_str()) {
        return Err("不支持的配置格式".into());
    }
    let path = PathBuf::from(&path);
    if !path.is_absolute() {
        return Err("请提供完整的配置文件路径".into());
    }
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("无法识别文件名")?;
    // 只收编文件名明确的 MCP 配置，避免把任意 JSON 当作服务来源。
    if ![
        "mcp.json",
        ".mcp.json",
        "mcp_config.json",
        "mcp-config.json",
        "cline_mcp_settings.json",
        "settings.json",
        "config.toml",
        // Zcode 的用户级 MCP 写在 cli/config.json 的 mcp.servers。
        "config.json",
    ]
    .contains(&file_name)
    {
        return Err("仅支持收编明确的 MCP 配置文件".into());
    }
    if path.starts_with(
        state
            .handle
            .path()
            .app_config_dir()
            .map_err(|e| e.to_string())?,
    ) {
        return Err("不能收编本应用的数据文件".into());
    }
    let raw = read_optional(&path)?;
    let doc = parse(raw.as_deref(), &format)?;
    let map = entries(&doc, &format)?;
    if map.is_empty() {
        return Ok("该配置中没有可收编的 MCP 服务".into());
    }
    let mut data = load(&state)?;
    let mut added = 0;
    let mut conflicts = 0;
    let mut skipped = 0;
    for (name, config) in map {
        let config = normalize(config, &format);
        let mut target_name = name.clone();
        if data.servers.contains_key(&target_name) {
            target_name = format!("{name}-{agent_id}");
            if data.servers.contains_key(&target_name) {
                conflicts += 1;
                continue;
            }
        }
        let server = Server {
            check_timeout_sec: None,
            name: target_name,
            config,
        };
        // 与手动收编同一标准：结构必须完整、不携带原工具的插件变量。
        if validate(&server.name, &server.config).is_err()
            || server.config.to_string().contains("${")
        {
            skipped += 1;
            continue;
        }
        data.servers.insert(server.name.clone(), server);
        added += 1;
    }
    if added > 0 {
        save(&state, &data)?;
    }
    let mut message = format!("已收编 {added} 个服务");
    if conflicts > 0 {
        message.push_str(&format!("，{conflicts} 个同名跳过"));
    }
    if skipped > 0 {
        message.push_str(&format!("，{skipped} 个含不可移植配置已跳过"));
    }
    Ok(message)
}

fn make_plan(
    agent_id: String,
    b: Binding,
    before: Option<Vec<u8>>,
    managed: BTreeMap<String, Value>,
    selected: Vec<Server>,
) -> Result<(Plan, Vec<Action>), String> {
    let doc = parse(before.as_deref(), &b.format)?;
    let mut map = entries(&doc, &b.format)?;
    let mut next = BTreeMap::new();
    for server in selected {
        next.insert(server.name.clone(), native(&server, &b.format)?);
    }
    let mut actions = Vec::new();
    for (name, old) in &managed {
        if !next.contains_key(name) {
            match map.get(name) {
                Some(current) if current != old => {
                    return Err(format!("{name} 已被外部修改，停止移除；请先收编当前配置"))
                }
                Some(_) => {
                    map.remove(name);
                    actions.push(Action {
                        name: name.clone(),
                        kind: "remove".into(),
                    });
                }
                None => {}
            }
        }
    }
    for (name, expected) in &next {
        let kind = match map.get(name) {
            None => "add",
            Some(current) if current == expected => "keep",
            Some(current) if managed.get(name) == Some(current) => "update",
            Some(_) => {
                return Err(format!(
                    "目标已存在同名 MCP「{name}」，内容不同；请先收编或改名，未覆盖原配置"
                ))
            }
        };
        actions.push(Action {
            name: name.clone(),
            kind: kind.into(),
        });
        map.insert(name.clone(), expected.clone());
    }
    let after = encode(before.as_deref(), &b.format, map)?;
    let ticket = format!(
        "{}-{}",
        super::now_ts(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    Ok((
        Plan {
            ticket,
            agent_id,
            binding: b,
            before,
            after,
            managed,
            next,
            check_library: true,
            paused_change: None,
        },
        actions,
    ))
}
fn remember_plan(plan: Plan) -> Result<(), String> {
    let mut plans = PLANS.lock().unwrap();
    if plans.len() >= 96 {
        plans.clear();
    }
    plans.insert(plan.ticket.clone(), plan);
    Ok(())
}

/// 直接编辑自己配置的服务。暂停会从 Agent 配置移出条目、保留原始定义，适用于没有原生禁用字段的客户端。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeRequest {
    agent_id: String,
    path: String,
    format: String,
    name: String,
    operation: String,
    config: Option<Value>,
    expected: Value,
    kind: Option<String>,
}
#[tauri::command]
pub fn mcp_preview_change(
    state: State<'_, AppState>,
    request: ChangeRequest,
) -> Result<Preview, String> {
    let ChangeRequest {
        agent_id,
        path,
        format,
        name,
        operation,
        config,
        expected,
        kind,
    } = request;
    let _guard = IO.lock().unwrap();
    let discovered = inventory(&state)?;
    let kind = kind.unwrap_or_else(|| {
        if operation == "resume" {
            "paused".into()
        } else {
            "user".into()
        }
    });
    let found = discovered
        .sources
        .iter()
        .find(|s| {
            s.agent_id == agent_id
                && s.format == format
                && s.path == path
                && ["user", "paused"].contains(&s.kind.as_str())
                && s.kind == kind
                && s.servers.iter().any(|x| x.name == name)
        })
        .ok_or("来源已变化，或不是可管理的用户配置，请重新扫描")?;
    let _ = found;
    let cfg = state.config.lock().unwrap();
    let inv = state.inventory.lock().unwrap();
    agent_policy::require_agent(&cfg, inv.as_ref(), &agent_id)?;
    if !supported_format(&format) {
        return Err("该来源格式尚未适配，请在原工具中修改".into());
    }
    let data = load(&state)?;
    let b = Binding {
        agent_id: agent_id.clone(),
        path: path.clone(),
        format: format.clone(),
    };
    let before = read_optional(Path::new(&path))?;
    let mut map = entries(&parse(before.as_deref(), &format)?, &format)?;
    let key = format!("{agent_id}:{path}");
    let paused_before = data.paused.get(&key).cloned();
    let mut paused = paused_before.clone().unwrap_or(PausedSource {
        binding: b.clone(),
        servers: BTreeMap::new(),
        owned: Vec::new(),
    });
    let was_owned = paused.owned.contains(&name);
    let current = (if kind == "paused" {
        paused.servers.get(&name)
    } else {
        map.get(&name)
    })
    .ok_or("服务已被移除，请重新扫描")?;
    if normalize(current.clone(), &format) != expected {
        return Err("服务配置已被外部修改，请重新扫描后再操作".into());
    }
    let original = current.clone();
    match operation.as_str() {
        "activate" => {
            let mut raw = map
                .get(&name)
                .cloned()
                .ok_or("服务已移出配置，请使用重新启用")?;
            if let Some(obj) = raw.as_object_mut() {
                obj.remove("disabled");
                if obj.contains_key("enabled") {
                    obj.insert("enabled".into(), json!(true));
                }
            }
            map.insert(name.clone(), raw);
        }
        "pause" => {
            if paused.servers.contains_key(&name) {
                return Err("已有同名的暂停定义，请先处理旧定义，未覆盖".into());
            }
            if map.remove(&name).is_none() {
                return Err("服务已暂停".into());
            }
            if data.managed.get(&agent_id).and_then(|m| m.get(&name)) == Some(&original)
                && !was_owned
            {
                paused.owned.push(name.clone());
            }
            paused.servers.insert(name.clone(), original.clone());
        }
        "resume" => {
            if map.contains_key(&name) {
                return Err("目标已有同名服务，不能覆盖".into());
            }
            let mut raw = paused.servers.remove(&name).ok_or("未找到暂停的服务")?;
            if let Some(obj) = raw.as_object_mut() {
                obj.remove("disabled");
                if obj.contains_key("enabled") {
                    obj.insert("enabled".into(), json!(true));
                }
            }
            map.insert(name.clone(), raw);
        }
        "edit" => {
            let mut value = config.ok_or("缺少服务配置")?;
            let mut flags = Map::new();
            if let Some(obj) = value.as_object_mut() {
                for key in ["disabled", "enabled"] {
                    if let Some(v) = obj.remove(key) {
                        flags.insert(key.into(), v);
                    }
                }
            }
            let mut updated = native(
                &Server {
                    check_timeout_sec: None,
                    name: name.clone(),
                    config: value,
                },
                &format,
            )?;
            if let Some(obj) = updated.as_object_mut() {
                obj.extend(flags);
            }
            if kind == "paused" {
                paused.servers.insert(name.clone(), updated);
            } else {
                map.insert(name.clone(), updated);
            }
        }
        "remove" => {
            if kind == "paused" {
                paused.servers.remove(&name);
            } else {
                map.remove(&name);
            }
        }
        _ => return Err("未知 MCP 操作".into()),
    }
    let managed = data.managed.get(&agent_id).cloned().unwrap_or_default();
    let mut next = managed.clone();
    let default_binding = data
        .bindings
        .get(&agent_id)
        .cloned()
        .or_else(|| binding(&agent_id));
    let is_default = default_binding
        .is_some_and(|b| super::known_agents::eq_path(Path::new(&b.path), Path::new(&path)));
    if operation == "resume" && was_owned {
        if is_default && data.servers.contains_key(&name) {
            if let Some(value) = map.get(&name) {
                next.insert(name.clone(), value.clone());
            }
        }
        paused.owned.retain(|n| n != &name);
    }
    if is_default && managed.get(&name) == Some(&original) {
        if let Some(value) = map.get(&name) {
            next.insert(name.clone(), value.clone());
        } else {
            next.remove(&name);
        }
    }
    let ticket = format!(
        "{}-{}",
        super::now_ts(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    );
    let preview = Preview {
        ticket: ticket.clone(),
        path: path.clone(),
        actions: vec![Action {
            name: name.clone(),
            kind: operation.clone(),
        }],
    };
    let after = encode(before.as_deref(), &format, map)?;
    remember_plan(Plan {
        ticket,
        agent_id,
        binding: b,
        before,
        after,
        managed,
        next,
        check_library: false,
        paused_change: Some((
            key,
            paused_before,
            if paused.servers.is_empty() {
                None
            } else {
                Some(paused)
            },
        )),
    })?;
    Ok(preview)
}
#[tauri::command]
pub fn mcp_preview(
    state: State<'_, AppState>,
    agent_id: String,
    names: Vec<String>,
) -> Result<Preview, String> {
    let _guard = IO.lock().unwrap();
    let data = load(&state)?;
    let cfg = state.config.lock().unwrap();
    let inv = state.inventory.lock().unwrap();
    agent_policy::require_agent(&cfg, inv.as_ref(), &agent_id)?;
    let b = data
        .bindings
        .get(&agent_id)
        .cloned()
        .or_else(|| binding(&agent_id))
        .ok_or("该工具尚未配置 MCP 文件位置，请先绑定")?;
    let paused_key = format!("{agent_id}:{}", b.path);
    if names.iter().any(|name| {
        data.paused
            .get(&paused_key)
            .is_some_and(|s| s.servers.contains_key(name))
    }) {
        return Err("存在同名暂停定义，请先在按 Agent 视图重新启用或移除，再分配新配置".into());
    }
    let selected = names
        .iter()
        .map(|name| {
            data.servers
                .get(name)
                .cloned()
                .ok_or_else(|| format!("统一库中没有 {name}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let before = read_optional(Path::new(&b.path))?;
    let managed = data.managed.get(&agent_id).cloned().unwrap_or_default();
    let (plan, actions) = make_plan(agent_id, b, before, managed, selected)?;
    let preview = Preview {
        ticket: plan.ticket.clone(),
        path: plan.binding.path.clone(),
        actions,
    };
    remember_plan(plan)?;
    Ok(preview)
}
#[tauri::command]
pub fn mcp_apply(state: State<'_, AppState>, ticket: String) -> Result<String, String> {
    let _guard = IO.lock().unwrap();
    let plan = PLANS
        .lock()
        .unwrap()
        .remove(&ticket)
        .ok_or("预览已失效，请重新预览")?;
    let cfg = state.config.lock().unwrap();
    let inv = state.inventory.lock().unwrap();
    agent_policy::require_agent(&cfg, inv.as_ref(), &plan.agent_id)?;
    let path = Path::new(&plan.binding.path);
    if read_optional(path)? != plan.before {
        return Err("预览后目标配置已变化，请重新预览，未写入".into());
    }
    let mut data = load(&state)?;
    let current_binding = data
        .bindings
        .get(&plan.agent_id)
        .cloned()
        .or_else(|| binding(&plan.agent_id));
    if data
        .managed
        .get(&plan.agent_id)
        .cloned()
        .unwrap_or_default()
        != plan.managed
    {
        return Err("预览后分配记录已变化，请重新预览".into());
    }
    if plan.check_library {
        let current_binding = current_binding.ok_or("绑定已变化")?;
        if current_binding.path != plan.binding.path
            || current_binding.format != plan.binding.format
        {
            return Err("预览后绑定已变化，请重新预览".into());
        }
    }
    for (name, expected) in plan.next.iter().filter(|_| plan.check_library) {
        let server = data
            .servers
            .get(name)
            .ok_or("预览后统一库已变化，请重新预览")?;
        if native(server, &plan.binding.format)? != *expected {
            return Err("预览后统一库已变化，请重新预览".into());
        }
    }
    if let Some((key, before, _)) = &plan.paused_change {
        if data.paused.get(key) != before.as_ref() {
            return Err("预览后暂停记录已变化，请重新预览".into());
        }
    }
    let backup = Backup {
        path: plan.binding.path.clone(),
        before: plan.before.clone(),
        after: plan.after.clone(),
        managed: plan.managed,
        paused_key: plan.paused_change.as_ref().map(|(key, _, _)| key.clone()),
        paused_before: plan
            .paused_change
            .as_ref()
            .and_then(|(_, before, _)| before.clone()),
    };
    // 先保存原始字节备份；之后即使写入失败也能核对与恢复。
    let backup_path =
        store_path(&state)?.with_file_name(format!("mcp-backup-{}.json", plan.ticket));
    atomic_write(
        &backup_path,
        &serde_json::to_vec(&backup).map_err(|e| e.to_string())?,
    )?;
    atomic_write(path, &plan.after)?;
    data.managed.insert(plan.agent_id.clone(), plan.next);
    if let Some((key, _, next)) = plan.paused_change {
        if let Some(next) = next {
            data.paused.insert(key, next);
        } else {
            data.paused.remove(&key);
        }
    }
    data.rollback.insert(plan.agent_id, backup);
    if let Err(error) = save(&state, &data) {
        let recovery = if let Some(bytes) = plan.before {
            atomic_write(path, &bytes)
        } else {
            fs::remove_file(path).map_err(|e| e.to_string())
        };
        return Err(format!(
            "管理记录保存失败：{error}；目标恢复：{}；备份：{}",
            if recovery.is_ok() {
                "成功"
            } else {
                "失败，请从备份恢复"
            },
            backup_path.display()
        ));
    }
    Ok("MCP 配置已写入，原配置已备份；请在目标工具中重新加载 MCP，必要时完成登录授权".into())
}
#[tauri::command]
pub fn mcp_restore(state: State<'_, AppState>, agent_id: String) -> Result<String, String> {
    let _guard = IO.lock().unwrap();
    let mut data = load(&state)?;
    let cfg = state.config.lock().unwrap();
    let inv = state.inventory.lock().unwrap();
    agent_policy::require_agent(&cfg, inv.as_ref(), &agent_id)?;
    let backup = data
        .rollback
        .get(&agent_id)
        .cloned()
        .ok_or("没有可恢复的 MCP 部署")?;
    let path = Path::new(&backup.path);
    if read_optional(path)?.as_deref() != Some(backup.after.as_slice()) {
        return Err("部署后配置已被外部修改，停止恢复以保护新增配置".into());
    }
    if let Some(before) = &backup.before {
        atomic_write(path, before)?;
    } else {
        fs::remove_file(path).map_err(|e| e.to_string())?;
    }
    data.managed
        .insert(agent_id.clone(), backup.managed.clone());
    if let Some(key) = &backup.paused_key {
        if let Some(before) = &backup.paused_before {
            data.paused.insert(key.clone(), before.clone());
        } else {
            data.paused.remove(key);
        }
    }
    data.rollback.remove(&agent_id);
    if let Err(error) = save(&state, &data) {
        atomic_write(path, &backup.after)?;
        return Err(error);
    }
    Ok("已恢复到上次 MCP 部署前的完整配置".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn b(format: &str) -> Binding {
        Binding {
            agent_id: "test".into(),
            path: "test.json".into(),
            format: format.into(),
        }
    }
    fn server(name: &str, config: Value) -> Server {
        Server {
            check_timeout_sec: None,
            name: name.into(),
            config,
        }
    }
    #[test]
    fn preserves_unrelated_servers_and_settings() {
        let before =
            br#"{"theme":"dark","mcpServers":{"external":{"url":"https://example.com"}}}"#.to_vec();
        let (p, _) = make_plan(
            "test".into(),
            b("json"),
            Some(before),
            BTreeMap::new(),
            vec![server("new", json!({"command":"npx","args":["demo"]}))],
        )
        .unwrap();
        let d: Value = serde_json::from_slice(&p.after).unwrap();
        assert_eq!(d["theme"], "dark");
        assert!(d["mcpServers"]["external"].is_object());
    }
    #[test]
    fn conflicts_and_external_edits_never_overwrite() {
        let before = br#"{"mcpServers":{"same":{"command":"original"}}}"#.to_vec();
        assert!(make_plan(
            "t".into(),
            b("json"),
            Some(before.clone()),
            BTreeMap::new(),
            vec![server("same", json!({"command":"replacement"}))]
        )
        .is_err());
        let old = BTreeMap::from([("same".into(), json!({"command":"old"}))]);
        assert!(make_plan("t".into(), b("json"), Some(before), old, vec![]).is_err());
    }
    #[test]
    fn removal_only_touches_owned_entries() {
        let before =
            br#"{"mcpServers":{"mine":{"command":"x"},"external":{"command":"y"}}}"#.to_vec();
        let old = BTreeMap::from([("mine".into(), json!({"command":"x"}))]);
        let (p, actions) = make_plan("t".into(), b("json"), Some(before), old, vec![]).unwrap();
        let d: Value = serde_json::from_slice(&p.after).unwrap();
        assert!(d["mcpServers"].get("mine").is_none());
        assert!(d["mcpServers"]["external"].is_object());
        assert_eq!(actions[0].kind, "remove");
    }
    #[test]
    fn codex_preserves_other_sections_and_quoted_names() {
        let before =
            b"# my settings\nmodel = \"test\" # keep comment\n[features]\nfoo = true\n".to_vec();
        let (p, _) = make_plan(
            "t".into(),
            b("codex"),
            Some(before),
            BTreeMap::new(),
            vec![server(
                "my.server",
                json!({"url":"https://example.com/mcp","headers":{"Authorization":"secret"}}),
            )],
        )
        .unwrap();
        let text = String::from_utf8(p.after.clone()).unwrap();
        assert!(text.contains("# keep comment"));
        let d = parse(Some(&p.after), "codex").unwrap();
        assert_eq!(d["features"]["foo"], true);
        assert_eq!(
            d["mcp_servers"]["my.server"]["http_headers"]["Authorization"],
            "secret"
        );
    }
    #[test]
    fn vscode_uses_servers_and_preserves_inputs() {
        let before = br#"{"inputs":[{"id":"key"}],"servers":{}}"#.to_vec();
        let (p, _) = make_plan(
            "t".into(),
            b("vscode"),
            Some(before),
            BTreeMap::new(),
            vec![server("demo", json!({"url":"https://example.com"}))],
        )
        .unwrap();
        let d: Value = serde_json::from_slice(&p.after).unwrap();
        assert_eq!(d["servers"]["demo"]["type"], "http");
        assert_eq!(d["inputs"][0]["id"], "key");
    }
    #[test]
    fn rejects_invalid_and_nonportable_configs() {
        assert!(validate("x", &json!({"command":"x","url":"https://x"})).is_err());
        assert!(validate("x", &json!({"command":"x","args":"bad"})).is_err());
        assert!(native(
            &server("x", json!({"command":"${CLAUDE_PLUGIN_ROOT}/run"})),
            "json"
        )
        .is_err());
        assert!(native(
            &server("x", json!({"command":"x","disabled":true})),
            "codex"
        )
        .is_err());
        assert!(native(
            &server("x", json!({"url":"https://x","type":"sse"})),
            "codex"
        )
        .is_err());
        assert!(parse(Some(b"{broken}"), "json").is_err());
    }
    #[test]
    fn jsonc_keeps_comments_unicode_and_url_strings() {
        let before = "{\n // 中文注释\n \"theme\": \"https://example.com/a/*b*/\",\n \"mcpServers\": {\"old\":{\"command\":\"x\"},}, // 保留行尾\n}\n";
        let (plan, _) = make_plan(
            "t".into(),
            b("json"),
            Some(before.as_bytes().to_vec()),
            BTreeMap::new(),
            vec![server("new", json!({"command":"y"}))],
        )
        .unwrap();
        let text = String::from_utf8(plan.after.clone()).unwrap();
        assert!(text.contains("// 中文注释"));
        assert!(text.contains("// 保留行尾"));
        let doc = parse(Some(&plan.after), "json").unwrap();
        assert_eq!(doc["theme"], "https://example.com/a/*b*/");
        assert_eq!(doc["mcpServers"].as_object().unwrap().len(), 2);
    }
    #[test]
    fn jsonc_inserts_section_before_existing_entries_and_comments() {
        for text in [
            "{\"theme\":true, // trailing comment\n}",
            "{/* only comment */}",
            "{}",
        ] {
            let (plan, _) = make_plan(
                "t".into(),
                b("json"),
                Some(text.as_bytes().to_vec()),
                BTreeMap::new(),
                vec![server("new", json!({"command":"y"}))],
            )
            .unwrap();
            let doc = parse(Some(&plan.after), "json").unwrap();
            assert!(doc["mcpServers"]["new"].is_object());
        }
    }
    #[test]
    fn gemini_http_transport_does_not_turn_into_sse() {
        let config = json!({"httpUrl":"https://example.com/mcp"});
        let s = server("remote", normalize(config.clone(), "gemini"));
        assert_eq!(native(&s, "gemini").unwrap(), config);
        assert_eq!(native(&s, "json").unwrap()["type"], "http");
        let sse = server(
            "sse",
            normalize(json!({"url":"https://example.com/sse"}), "gemini"),
        );
        assert_eq!(native(&sse, "json").unwrap()["type"], "sse");
        assert!(native(&sse, "codex").is_err());
        assert!(native(
            &server(
                "x",
                json!({"command":"run", "env":{"KEY":"${input:secret}"}})
            ),
            "json"
        )
        .is_err());
    }
    #[test]
    fn duplicate_root_sections_are_never_rewritten() {
        let bytes = br#"{"mcpServers":{},"mcpServers":{}}"#.to_vec();
        assert!(make_plan("t".into(), b("json"), Some(bytes), BTreeMap::new(), vec![]).is_err());
    }
    #[test]
    fn zcode_nested_node_is_replaced_and_siblings_preserved() {
        let before =
            br#"{"plugins":{"cache":true},"mcp":{"servers":{"old":{"command":"x"}}}}"#.to_vec();
        let (p, _) = make_plan(
            "t".into(),
            b("zcode"),
            Some(before),
            BTreeMap::new(),
            vec![server("new", json!({"command":"npx","args":["demo"]}))],
        )
        .unwrap();
        let d: Value = serde_json::from_slice(&p.after).unwrap();
        assert_eq!(d["plugins"]["cache"], true, "mcp 外的节点必须原样保留");
        assert!(
            d["mcp"]["servers"]["old"].is_object(),
            "未分配的现有服务保留"
        );
        assert_eq!(d["mcp"]["servers"]["new"]["type"], "stdio");
        assert_eq!(d["mcp"]["servers"]["new"]["command"], "npx");
    }
    #[test]
    fn zcode_inserts_missing_nodes_at_every_depth() {
        for before in [
            "{}",
            r#"{"plugins":{}}"#,
            r#"{"mcp":{"other":1},"plugins":{}}"#,
            r#"{"mcp":{}}"#,
        ] {
            let (p, _) = make_plan(
                "t".into(),
                b("zcode"),
                Some(before.as_bytes().to_vec()),
                BTreeMap::new(),
                vec![server("demo", json!({"url":"https://example.com/mcp"}))],
            )
            .unwrap();
            let d: Value = serde_json::from_slice(&p.after).unwrap();
            assert_eq!(d["mcp"]["servers"]["demo"]["type"], "http");
            assert_eq!(
                d["mcp"]["servers"]["demo"]["url"],
                "https://example.com/mcp"
            );
            if before.contains("plugins") {
                assert_eq!(d["plugins"], json!({}), "根级其他键保留");
            }
        }
        let (p, _) = make_plan(
            "t".into(),
            b("zcode"),
            Some(br#"{"mcp":{"other":1}}"#.to_vec()),
            BTreeMap::new(),
            vec![server("d", json!({"command":"x"}))],
        )
        .unwrap();
        let d: Value = serde_json::from_slice(&p.after).unwrap();
        assert_eq!(d["mcp"]["other"], 1, "mcp 内的其他键保留");
    }
    #[test]
    fn zcode_rejects_fields_the_client_silently_drops() {
        assert!(native(
            &server("x", json!({"command":"x","startup_timeout_sec":120})),
            "zcode"
        )
        .is_err());
        assert!(native(
            &server("x", json!({"command":"x","env_vars":["A"]})),
            "zcode"
        )
        .is_err());
        assert!(native(
            &server("x", json!({"command":"x","timeoutMs":5000})),
            "zcode"
        )
        .is_ok());
        assert_eq!(
            native(&server("x", json!({"url":"https://e.com"})), "zcode").unwrap()["type"],
            "http"
        );
        assert_eq!(
            native(
                &server("x", json!({"url":"https://e.com","type":"streamable-http"})),
                "zcode"
            )
            .unwrap()["type"],
            "http"
        );
    }
    #[test]
    fn zcode_duplicate_and_non_object_nodes_are_rejected() {
        let bytes = br#"{"mcp":{"servers":{},"servers":{}}}"#.to_vec();
        assert!(make_plan("t".into(), b("zcode"), Some(bytes), BTreeMap::new(), vec![]).is_err());
        let not_object = br#"{"mcp":{"servers":[]}}"#.to_vec();
        assert!(entries(&parse(Some(&not_object), "zcode").unwrap(), "zcode").is_err());
    }
    #[test]
    fn local_configuration_scan_is_read_only() {
        let mut total = 0;
        for id in [
            "codex",
            "claude",
            "zcode",
            "cline",
            "kimi-code",
            "gemini",
            "qwen",
            "cursor",
            "windsurf",
            "qoder-cn",
            "copilot",
        ] {
            let Some(b) = binding(id) else {
                continue;
            };
            let path = Path::new(&b.path);
            if !path.is_file() {
                continue;
            }
            let before = fs::read(path).unwrap();
            let found = source(&b, id, "user");
            assert!(found.error.is_empty(), "{id} 读取失败：{}", found.error);
            assert_eq!(before, fs::read(path).unwrap());
            println!("MCP {id}: {} records (read only)", found.servers.len());
            total += found.servers.len();
        }
        println!("MCP local records total: {total}");
    }
    #[test]
    fn hosted_mcp_cannot_be_deployed_as_a_native_server() {
        let hosted = server("hosted", serde_json::json!({"_hub_hosted":true}));
        assert!(native(&hosted, "json").is_err());
    }

}
