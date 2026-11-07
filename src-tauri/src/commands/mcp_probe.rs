//! 有界的 MCP 连通检测：只握手与列出工具，不执行 tools/call。
use super::mcp::Server;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    process::{Child, ChildStdin, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use url::Url;

const MAX_BYTES: usize = 8 * 1024 * 1024;
const VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
static ACTIVE: Mutex<BTreeMap<String, Arc<AtomicBool>>> = Mutex::new(BTreeMap::new());

#[derive(Deserialize)]
pub struct ProbeRequest {
    pub id: String,
    pub server: Server,
}
#[derive(Serialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}
#[derive(Serialize)]
pub struct ProbeResult {
    pub state: String,
    pub code: String,
    pub phase: String,
    pub message: String,
    pub details: String,
    pub server_name: String,
    pub server_version: String,
    pub protocol: String,
    pub tool_count: usize,
    pub tools: Vec<ToolInfo>,
    pub elapsed_ms: u128,
    pub checked_at: u64,
    pub partial: bool,
}
#[derive(Debug)]
struct Failure {
    code: &'static str,
    message: String,
}
type Result<T> = std::result::Result<T, Failure>;
fn fail(code: &'static str, message: impl Into<String>) -> Failure {
    Failure {
        code,
        message: message.into(),
    }
}

struct Scope {
    deadline: Instant,
    cancel: Arc<AtomicBool>,
}
impl Scope {
    fn check(&self) -> Result<()> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(fail("cancelled", "检测已停止"));
        }
        if Instant::now() >= self.deadline {
            return Err(fail(
                "timeout",
                "检测超时：服务未在设定时间内完成启动、握手及工具发现",
            ));
        }
        Ok(())
    }
    fn wait(&self) -> Result<Duration> {
        self.check()?;
        Ok(self
            .deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(80)))
    }
}

/// 关闭 Job 会清理本次启动的整个进程树，不影响用户已运行的 Agent。
#[cfg(windows)]
pub(super) struct ProcessJob(windows_sys::Win32::Foundation::HANDLE);
#[cfg(windows)]
impl ProcessJob {
    pub(super) fn try_attach(child: &Child) -> Option<Self> {
        Self::attach(child).ok()
    }

    fn attach(child: &Child) -> Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::{Foundation::CloseHandle, System::JobObjects::*};
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return Err(fail("process_guard", "无法创建检测进程组"));
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(handle, child.as_raw_handle()) == 0
            {
                CloseHandle(handle);
                return Err(fail("process_guard", "无法管理检测进程，已停止启动"));
            }
            Ok(Self(handle))
        }
    }
}
#[cfg(windows)]
impl Drop for ProcessJob {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

struct Process {
    child: Child,
    input: Option<ChildStdin>,
    rx: mpsc::Receiver<Vec<u8>>,
    buf: Vec<u8>,
    eof: bool,
    stderr: Arc<Mutex<Vec<u8>>>,
    threads: Vec<std::thread::JoinHandle<()>>,
    #[cfg(windows)]
    job: Option<ProcessJob>,
}
impl Process {
    fn spawn(mut command: std::process::Command) -> Result<Self> {
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                fail(
                    if e.kind() == std::io::ErrorKind::NotFound {
                        "missing_command"
                    } else {
                        "start_failed"
                    },
                    if e.kind() == std::io::ErrorKind::NotFound {
                        "找不到启动命令，请检查 Node.js、npx、uvx 或可执行文件路径".into()
                    } else {
                        format!("进程启动失败：{e}")
                    },
                )
            })?;
        #[cfg(windows)]
        let job = match ProcessJob::attach(&child) {
            Ok(job) => Some(job),
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e);
            }
        };
        let input = child.stdin.take();
        let mut stdout = child.stdout.take().unwrap();
        let mut stderr = child.stderr.take().unwrap();
        let (tx, rx) = mpsc::sync_channel(256);
        let output = std::thread::spawn(move || {
            let mut bytes = [0; 4096];
            loop {
                match stdout.read(&mut bytes) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if tx.send(bytes[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                }
            }
        });
        let errors = Arc::new(Mutex::new(Vec::<u8>::new()));
        let captured = errors.clone();
        let error_thread = std::thread::spawn(move || {
            let mut bytes = [0; 2048];
            while let Ok(n) = stderr.read(&mut bytes) {
                if n == 0 {
                    break;
                }
                let mut tail = captured.lock().unwrap();
                tail.extend_from_slice(&bytes[..n]);
                if tail.len() > 16384 {
                    let end = tail.len() - 16384;
                    tail.drain(..end);
                }
            }
        });
        Ok(Self {
            child,
            input,
            rx,
            buf: Vec::new(),
            eof: false,
            stderr: errors,
            threads: vec![output, error_thread],
            #[cfg(windows)]
            job,
        })
    }
    fn send(&mut self, value: &Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(value).unwrap();
        bytes.push(b'\n');
        self.input
            .as_mut()
            .ok_or_else(|| fail("closed", "服务已关闭输入"))?
            .write_all(&bytes)
            .map_err(|_| fail("closed", "服务在握手过程中退出或关闭了输入"))
    }
    fn line(&mut self, scope: &Scope) -> Result<Option<String>> {
        loop {
            scope.check()?;
            if let Some(n) = self.buf.iter().position(|b| *b == b'\n') {
                let bytes: Vec<u8> = self.buf.drain(..=n).collect();
                return String::from_utf8(bytes)
                    .map(Some)
                    .map_err(|_| fail("protocol_error", "服务返回了非 UTF-8 内容"));
            }
            if self.eof {
                if self.buf.is_empty() {
                    return Ok(None);
                }
                let bytes = std::mem::take(&mut self.buf);
                return String::from_utf8(bytes)
                    .map(Some)
                    .map_err(|_| fail("protocol_error", "服务返回了非 UTF-8 内容"));
            }
            match self.rx.recv_timeout(scope.wait()?) {
                Ok(bytes) => {
                    self.buf.extend(bytes);
                    if self.buf.len() > MAX_BYTES {
                        return Err(fail("response_too_large", "服务响应超过 8 MB，已停止检测"));
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => self.eof = true,
            }
        }
    }
    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.stderr.lock().unwrap()).into_owned()
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.input.take();
        #[cfg(windows)]
        {
            self.job.take();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        // 丢弃接收端，解除输出线程的有界队列等待。
        let (_, dummy) = mpsc::channel();
        let old = std::mem::replace(&mut self.rx, dummy);
        drop(old);
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
    }
}

struct Resolved {
    config: Value,
    secrets: Vec<String>,
}
fn expand(text: &str) -> Result<String> {
    let mut out = String::new();
    let mut remaining = text;
    while let Some(at) = remaining.find("${") {
        out.push_str(&remaining[..at]);
        let tail = &remaining[at + 2..];
        let end = tail
            .find('}')
            .ok_or_else(|| fail("missing_env", "环境变量引用未闭合"))?;
        let name = tail[..end].strip_prefix("env:").unwrap_or(&tail[..end]);
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(fail(
                "missing_env",
                "配置依赖原 Agent 的交互输入或插件变量，请改用本机环境变量或明确的参数值",
            ));
        }
        let value = std::env::var(name)
            .map_err(|_| fail("missing_env", format!("缺少环境变量：{name}")))?;
        out.push_str(&value);
        remaining = &tail[end + 1..];
    }
    out.push_str(remaining);
    if out.contains("{{") {
        return Err(fail(
            "missing_env",
            "配置依赖插件变量，本工具无法取得其运行环境",
        ));
    }
    Ok(out)
}
fn resolve(server: &Server) -> Result<Resolved> {
    if server.config.to_string().len() > MAX_BYTES {
        return Err(fail("invalid_config", "配置超过 8 MB，停止检测"));
    }
    let mut config = server.config.clone();
    let obj = config
        .as_object_mut()
        .ok_or_else(|| fail("invalid_config", "服务配置必须是对象"))?;
    if obj.contains_key("_hub_hosted") {
        return Err(fail(
            "hosted_only",
            "宿主托管连接器没有独立连接配置，请在原应用中验证",
        ));
    }
    if obj.get("command").is_some() == obj.get("url").is_some() {
        return Err(fail("invalid_config", "请填写启动命令或服务地址中的一种"));
    }
    let mut secrets = Vec::new();
    for key in ["env", "headers", "http_headers", "env_http_headers"] {
        if let Some(map) = obj.get(key).and_then(Value::as_object) {
            for v in map.values().filter_map(Value::as_str) {
                if v.len() >= 4 {
                    secrets.push(v.to_string());
                }
            }
        }
    }
    if let Some(raw) = obj.remove("http_headers") {
        obj.entry("headers").or_insert(raw);
    }
    if let Some(map) = obj
        .remove("env_http_headers")
        .and_then(|v| v.as_object().cloned())
    {
        let mut headers = obj
            .get("headers")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        for (k, v) in map {
            let name = v
                .as_str()
                .ok_or_else(|| fail("invalid_config", "环境请求头必须引用环境变量名"))?;
            let value = std::env::var(name)
                .map_err(|_| fail("missing_env", format!("缺少环境变量：{name}")))?;
            secrets.push(value.clone());
            headers.insert(k, json!(value));
        }
        obj.insert("headers".into(), json!(headers));
    }
    if let Some(name) = obj.get("bearer_token_env_var").and_then(Value::as_str) {
        let token = std::env::var(name)
            .map_err(|_| fail("missing_env", format!("缺少环境变量：{name}")))?;
        secrets.push(token.clone());
        let mut headers = obj
            .get("headers")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        headers
            .entry("Authorization")
            .or_insert(json!(format!("Bearer {token}")));
        obj.insert("headers".into(), json!(headers));
    }
    for key in ["command", "cwd", "url"] {
        if let Some(v) = obj.get(key).and_then(Value::as_str) {
            obj.insert(key.into(), json!(expand(v)?));
        }
    }
    if let Some(args) = obj.get_mut("args") {
        let list = args
            .as_array_mut()
            .ok_or_else(|| fail("invalid_config", "启动参数必须是字符串数组"))?;
        for v in list {
            let text = v
                .as_str()
                .ok_or_else(|| fail("invalid_config", "启动参数必须是字符串"))?;
            *v = json!(expand(text)?);
        }
    }
    for key in ["env", "headers"] {
        if let Some(map) = obj.get_mut(key) {
            let map = map
                .as_object_mut()
                .ok_or_else(|| fail("invalid_config", "环境变量和请求头必须是键值对象"))?;
            for v in map.values_mut() {
                let value =
                    expand(v.as_str().ok_or_else(|| {
                        fail("invalid_config", "环境变量和请求头值必须是字符串")
                    })?)?;
                if value.len() >= 4 {
                    secrets.push(value.clone());
                }
                *v = json!(value);
            }
        }
    }
    if let Some(headers) = obj.get("headers").and_then(Value::as_object) {
        for value in headers.values().filter_map(Value::as_str) {
            if let Some((scheme, token)) = value.split_once(' ') {
                if ["bearer", "basic"]
                    .iter()
                    .any(|s| scheme.eq_ignore_ascii_case(s))
                    && token.len() >= 4
                {
                    secrets.push(token.into());
                }
            }
        }
    }
    if let Some(args) = obj.get("args").and_then(Value::as_array) {
        let mut sensitive = false;
        for value in args.iter().filter_map(Value::as_str) {
            if sensitive && value.len() >= 4 {
                secrets.push(value.into());
            }
            let lower = value.to_ascii_lowercase();
            sensitive = [
                "token",
                "secret",
                "password",
                "api-key",
                "api_key",
                "authorization",
            ]
            .iter()
            .any(|k| lower.contains(k));
            if sensitive {
                if let Some((_, token)) = value.split_once('=') {
                    if token.len() >= 4 {
                        secrets.push(token.into());
                    }
                }
            }
        }
    }
    if let Some(url) = obj.get("url").and_then(Value::as_str) {
        let parsed = Url::parse(url)
            .map_err(|_| fail("invalid_url", "服务地址不是有效的 HTTP/HTTPS URL"))?;
        if !["http", "https"].contains(&parsed.scheme()) {
            return Err(fail("invalid_url", "服务地址必须使用 HTTP 或 HTTPS"));
        }
        if let Some(q) = parsed.query() {
            for (_, v) in url::form_urlencoded::parse(q.as_bytes()) {
                if v.len() >= 4 {
                    secrets.push(v.into_owned());
                }
            }
        }
        if let Some(p) = parsed.password() {
            secrets.push(p.into());
        }
    }
    Ok(Resolved { config, secrets })
}
fn redact(text: &str, secrets: &[String]) -> String {
    let mut result = text.to_string();
    let mut sorted = secrets.to_vec();
    sorted.sort_by_key(|v| std::cmp::Reverse(v.len()));
    for s in sorted {
        if s.len() >= 4 {
            result = result.replace(&s, "[已隐藏]");
        }
    }
    result.chars().take(4000).collect()
}

struct HttpMeta {
    status: u16,
    headers: BTreeMap<String, String>,
}
fn http_headers(process: &mut Process, scope: &Scope) -> Result<HttpMeta> {
    let mut meta = HttpMeta {
        status: 0,
        headers: BTreeMap::new(),
    };
    let mut size = 0;
    loop {
        let line = process.line(scope)?.ok_or_else(|| {
            fail(
                "network_error",
                format!("远程连接未返回 HTTP 响应。{}", process.stderr()),
            )
        })?;
        size += line.len();
        if size > 65536 {
            return Err(fail("protocol_error", "HTTP 响应头过大"));
        }
        let line = line.trim_end();
        if line.starts_with("HTTP/") {
            meta.status = line
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            meta.headers.clear();
        } else if line.is_empty() {
            if (100..200).contains(&meta.status) || meta.headers.contains_key(":proxy") {
                meta.status = 0;
                meta.headers.clear();
                continue;
            }
            if meta.status != 0 {
                break;
            }
        } else if let Some((k, v)) = line.split_once(':') {
            meta.headers.insert(k.to_ascii_lowercase(), v.trim().into());
        }
        if line.to_ascii_lowercase().contains("connection established") {
            meta.headers.insert(":proxy".into(), "1".into());
        }
    }
    match meta.status {
        200..=299 => Ok(meta),
        401 | 403 => Err(fail(
            "auth_required",
            format!(
                "远程服务返回 HTTP {}，请检查令牌、权限或完成 OAuth 授权",
                meta.status
            ),
        )),
        300..=399 => Err(fail(
            "redirect",
            "服务要求重定向，请核对最终 MCP 地址；未把凭证转发给其他地址",
        )),
        _ => Err(fail(
            "http_error",
            format!(
                "远程服务返回 HTTP {}，请检查端点路径和服务状态",
                meta.status
            ),
        )),
    }
}
fn quote_config(s: &str) -> String {
    format!(
        "\"{}\"",
        s.replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\r', "\\r")
            .replace('\n', "\\n")
    )
}
fn http_start(
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&Value>,
    session: Option<&str>,
    version: Option<&str>,
    scope: &Scope,
) -> Result<(Process, HttpMeta)> {
    http_request(
        if body.is_some() { "POST" } else { "GET" },
        url,
        headers,
        body,
        session,
        version,
        scope,
    )
}
fn http_request(
    method: &str,
    url: &str,
    headers: &BTreeMap<String, String>,
    body: Option<&Value>,
    session: Option<&str>,
    version: Option<&str>,
    scope: &Scope,
) -> Result<(Process, HttpMeta)> {
    scope.check()?;
    let mut config = format!(
        "url = {}\nrequest = {}\nheader = \"Accept: application/json, text/event-stream\"\n",
        quote_config(url),
        quote_config(method)
    );
    for (k, v) in headers {
        if k.is_empty()
            || !k
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
            || v.contains(['\r', '\n'])
        {
            return Err(fail("invalid_config", "请求头名称或值中包含无效字符"));
        }
        config.push_str(&format!(
            "header = {}\n",
            quote_config(&format!("{k}: {v}"))
        ));
    }
    if let Some(s) = session {
        if s.contains(['\r', '\n']) {
            return Err(fail("protocol_error", "会话标识包含无效字符"));
        }
        config.push_str(&format!(
            "header = {}\n",
            quote_config(&format!("Mcp-Session-Id: {s}"))
        ));
    }
    if let Some(v) = version {
        config.push_str(&format!(
            "header = {}\n",
            quote_config(&format!("MCP-Protocol-Version: {v}"))
        ));
    }
    if let Some(body) = body {
        config.push_str("header = \"Content-Type: application/json\"\n");
        config.push_str(&format!("data = {}\n", quote_config(&body.to_string())));
    }
    let mut command = super::hidden_command("curl");
    command.args([
        "--config",
        "-",
        "--silent",
        "--show-error",
        "--no-buffer",
        "--include",
        "--http1.1",
        "--proto",
        "=http,https",
        "--connect-timeout",
        "10",
        "--max-time",
        &scope
            .deadline
            .saturating_duration_since(Instant::now())
            .as_secs_f64()
            .max(0.1)
            .to_string(),
    ]);
    let mut process = Process::spawn(command)?;
    process
        .input
        .as_mut()
        .unwrap()
        .write_all(config.as_bytes())
        .map_err(|_| fail("network_error", "无法发送连接参数"))?;
    process.input.take();
    let meta = http_headers(&mut process, scope)?;
    Ok((process, meta))
}
fn sse_event(process: &mut Process, scope: &Scope) -> Result<Option<(String, String)>> {
    let mut event = String::new();
    let mut data = String::new();
    while let Some(line) = process.line(scope)? {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() {
            if !data.is_empty() {
                return Ok(Some((event, data.trim_end_matches('\n').into())));
            }
            continue;
        }
        if let Some(v) = line.strip_prefix("event:") {
            event = v.trim_start().into();
        } else if let Some(v) = line.strip_prefix("data:") {
            data.push_str(v.trim_start_matches(' '));
            data.push('\n');
            if data.len() > MAX_BYTES {
                return Err(fail("response_too_large", "SSE 事件超过大小限制"));
            }
        }
    }
    if data.is_empty() {
        Ok(None)
    } else {
        Ok(Some((event, data.trim_end_matches('\n').into())))
    }
}
fn server_response(value: &Value) -> Option<Value> {
    if value.get("method").is_none() || value.get("id").is_none() {
        return None;
    }
    Some(if value["method"] == "ping" {
        json!({"jsonrpc":"2.0","id":value["id"],"result":{}})
    } else {
        json!({"jsonrpc":"2.0","id":value["id"],"error":{"code":-32601,"message":"This diagnostic client does not provide interactive operations"}})
    })
}
fn rpc_result(value: Value, id: u64) -> Result<Option<Value>> {
    if value.get("method").is_some() {
        return Ok(None);
    }
    if value.get("id") != Some(&json!(id)) {
        return Ok(None);
    }
    if value.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
        return Err(fail("protocol_error", "服务返回的消息不是 JSON-RPC 2.0"));
    }
    if let Some(error) = value.get("error") {
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("服务器拒绝了请求");
        let lower = message.to_ascii_lowercase();
        return Err(fail(
            if lower.contains("unauthorized")
                || lower.contains("authentication")
                || lower.contains("invalid token")
            {
                "auth_required"
            } else {
                "rpc_error"
            },
            format!("MCP 请求失败：{message}"),
        ));
    }
    value
        .get("result")
        .cloned()
        .map(Some)
        .ok_or_else(|| fail("protocol_error", "MCP 响应缺少 result"))
}

enum Transport {
    Stdio(Process),
    Http {
        url: String,
        headers: BTreeMap<String, String>,
        session: Option<String>,
        version: Option<String>,
    },
    Sse {
        stream: Process,
        endpoint: String,
        headers: BTreeMap<String, String>,
    },
}
impl Transport {
    fn open(config: &Value, scope: &Scope) -> Result<Self> {
        scope.check()?;
        if let Some(command) = config.get("command").and_then(Value::as_str) {
            if command.trim().is_empty() {
                return Err(fail("invalid_config", "启动命令为空"));
            }
            let mut cmd = super::hidden_command(command);
            if let Some(args) = config.get("args").and_then(Value::as_array) {
                cmd.args(args.iter().filter_map(Value::as_str));
            }
            if let Some(env) = config.get("env").and_then(Value::as_object) {
                for (k, v) in env {
                    cmd.env(k, v.as_str().unwrap());
                }
            }
            if let Some(vars) = config.get("env_vars").and_then(Value::as_array) {
                for name in vars.iter().filter_map(Value::as_str) {
                    if std::env::var_os(name).is_none() {
                        return Err(fail("missing_env", format!("缺少环境变量：{name}")));
                    }
                }
            }
            if let Some(cwd) = config.get("cwd").and_then(Value::as_str) {
                let path = std::path::Path::new(cwd);
                if !path.is_absolute() || !path.is_dir() {
                    return Err(fail("invalid_cwd", "工作目录不存在，或不是绝对路径"));
                }
                cmd.current_dir(path);
            }
            return Ok(Self::Stdio(Process::spawn(cmd)?));
        }
        let url = config
            .get("url")
            .and_then(Value::as_str)
            .ok_or_else(|| fail("invalid_config", "缺少服务地址"))?
            .to_string();
        let headers = config
            .get("headers")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.as_str().unwrap().into()))
                    .collect()
            })
            .unwrap_or_default();
        if config.get("type").and_then(Value::as_str) == Some("sse") {
            let (mut stream, meta) = http_start(&url, &headers, None, None, None, scope)?;
            if !meta
                .headers
                .get("content-type")
                .is_some_and(|v| v.to_ascii_lowercase().starts_with("text/event-stream"))
            {
                return Err(fail(
                    "protocol_error",
                    "SSE 地址未返回事件流，请确认地址和传输方式",
                ));
            }
            while let Some((event, data)) = sse_event(&mut stream, scope)? {
                if event == "endpoint" {
                    let base = Url::parse(&url).unwrap();
                    let endpoint = base
                        .join(&data)
                        .map_err(|_| fail("protocol_error", "SSE 返回了无效的消息地址"))?;
                    if endpoint.origin() != base.origin() {
                        return Err(fail(
                            "protocol_error",
                            "SSE 消息端点跨域，未向其他主机发送凭证",
                        ));
                    }
                    return Ok(Self::Sse {
                        stream,
                        endpoint: endpoint.to_string(),
                        headers,
                    });
                }
            }
            return Err(fail("protocol_error", "SSE 连接未提供消息端点"));
        }
        Ok(Self::Http {
            url,
            headers,
            session: None,
            version: None,
        })
    }
    fn send(&mut self, message: &Value, scope: &Scope) -> Result<Option<Value>> {
        let id = message.get("id").and_then(Value::as_u64);
        match self {
            Self::Stdio(process) => {
                process.send(message)?;
                if let Some(id) = id {
                    let mut lines = 0;
                    while let Some(line) = process.line(scope)? {
                        if line.trim().is_empty() {
                            continue;
                        }
                        match serde_json::from_str::<Value>(&line) {
                            Ok(value) => {
                                if let Some(response) = server_response(&value) {
                                    process.send(&response)?;
                                }
                                if let Some(result) = rpc_result(value, id)? {
                                    return Ok(Some(result));
                                }
                            }
                            Err(_) => {
                                lines += 1;
                                if lines > 30 {
                                    return Err(fail(
                                        "protocol_error",
                                        "服务标准输出不是 MCP 消息，请检查启动命令",
                                    ));
                                }
                            }
                        }
                    }
                    return Err(fail(
                        "process_exit",
                        format!("MCP 进程在响应前退出。{}", process.stderr()),
                    ));
                }
                Ok(None)
            }
            Self::Http {
                url,
                headers,
                session,
                version,
            } => {
                let (mut process, meta) = http_start(
                    url,
                    headers,
                    Some(message),
                    session.as_deref(),
                    version.as_deref(),
                    scope,
                )?;
                if let Some(value) = meta.headers.get("mcp-session-id") {
                    *session = Some(value.clone());
                }
                let Some(id) = id else { return Ok(None) };
                if meta
                    .headers
                    .get("content-type")
                    .is_some_and(|v| v.to_ascii_lowercase().starts_with("text/event-stream"))
                {
                    while let Some((_, data)) = sse_event(&mut process, scope)? {
                        let value = serde_json::from_str(&data)
                            .map_err(|_| fail("protocol_error", "远程事件不是有效的 MCP JSON"))?;
                        if let Some(response) = server_response(&value) {
                            let _ = http_start(
                                url,
                                headers,
                                Some(&response),
                                session.as_deref(),
                                version.as_deref(),
                                scope,
                            )?;
                        }
                        if let Some(result) = rpc_result(value, id)? {
                            return Ok(Some(result));
                        }
                    }
                } else {
                    let mut body = String::new();
                    while let Some(line) = process.line(scope)? {
                        body.push_str(&line);
                        if body.len() > MAX_BYTES {
                            return Err(fail("response_too_large", "远程响应超过 8 MB"));
                        }
                    }
                    let value = serde_json::from_str(&body).map_err(|_| {
                        fail(
                            "protocol_error",
                            "地址可以访问，但未返回 MCP JSON；请检查是否填成网页地址",
                        )
                    })?;
                    if let Some(result) = rpc_result(value, id)? {
                        return Ok(Some(result));
                    }
                }
                Err(fail("protocol_error", "远程服务未返回匹配的 MCP 响应"))
            }
            Self::Sse {
                stream,
                endpoint,
                headers,
            } => {
                let _ = http_start(endpoint, headers, Some(message), None, None, scope)?;
                if let Some(id) = id {
                    while let Some((event, data)) = sse_event(stream, scope)? {
                        if event == "message" || event.is_empty() {
                            let value = serde_json::from_str(&data)
                                .map_err(|_| fail("protocol_error", "SSE 消息不是有效的 JSON"))?;
                            if let Some(response) = server_response(&value) {
                                let _ = http_start(
                                    endpoint,
                                    headers,
                                    Some(&response),
                                    None,
                                    None,
                                    scope,
                                )?;
                            }
                            if let Some(result) = rpc_result(value, id)? {
                                return Ok(Some(result));
                            }
                        }
                    }
                    return Err(fail("closed", "SSE 服务在响应前关闭"));
                }
                Ok(None)
            }
        }
    }
    fn version(&mut self, protocol: String) {
        if let Self::Http { version, .. } = self {
            *version = Some(protocol);
        }
    }
    fn close(&mut self, scope: &Scope) {
        if let Self::Http {
            url,
            headers,
            session: Some(session),
            version,
        } = self
        {
            let cleanup = Scope {
                deadline: scope
                    .deadline
                    .min(Instant::now() + Duration::from_millis(700)),
                cancel: scope.cancel.clone(),
            };
            // 部分服务不支持 DELETE，清理失败不覆盖已经取得的检测结果。
            let _ = http_request(
                "DELETE",
                url,
                headers,
                None,
                Some(session),
                version.as_deref(),
                &cleanup,
            );
        }
    }
}

fn run(server: &Server, cancel: Arc<AtomicBool>) -> ProbeResult {
    let start = Instant::now();
    let seconds = server
        .check_timeout_sec
        .or_else(|| {
            server
                .config
                .get("startup_timeout_sec")
                .and_then(Value::as_u64)
        })
        .unwrap_or(30)
        .clamp(1, 300);
    let scope = Scope {
        deadline: start + Duration::from_secs(seconds),
        cancel,
    };
    let mut out = ProbeResult {
        state: "failed".into(),
        code: String::new(),
        phase: "prepare".into(),
        message: String::new(),
        details: String::new(),
        server_name: String::new(),
        server_version: String::new(),
        protocol: String::new(),
        tool_count: 0,
        tools: Vec::new(),
        elapsed_ms: 0,
        checked_at: 0,
        partial: false,
    };
    let mut secrets = Vec::new();
    let mut connection = None;
    let result = (|| -> Result<()> {
        let resolved = resolve(server)?;
        secrets = resolved.secrets;
        out.phase = "connect".into();
        connection = Some(Transport::open(&resolved.config, &scope)?);
        let transport = connection.as_mut().unwrap();
        out.phase = "initialize".into();
        let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"AgentSkillHub diagnostic","version":env!("CARGO_PKG_VERSION")}}});
        let info = transport
            .send(&initialize, &scope)?
            .ok_or_else(|| fail("protocol_error", "服务没有返回初始化信息"))?;
        let protocol = info
            .get("protocolVersion")
            .and_then(Value::as_str)
            .ok_or_else(|| fail("protocol_error", "握手响应缺少协议版本"))?;
        if !VERSIONS.contains(&protocol) {
            return Err(fail(
                "unsupported_protocol",
                format!("服务协商的协议版本 {protocol} 暂不支持"),
            ));
        }
        out.protocol = protocol.into();
        if !info.get("capabilities").is_some_and(Value::is_object)
            || !info
                .pointer("/serverInfo/name")
                .is_some_and(Value::is_string)
            || !info
                .pointer("/serverInfo/version")
                .is_some_and(Value::is_string)
        {
            return Err(fail(
                "protocol_error",
                "握手响应缺少有效的服务标识或能力信息",
            ));
        }
        out.server_name = info
            .pointer("/serverInfo/name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into();
        out.server_version = info
            .pointer("/serverInfo/version")
            .and_then(Value::as_str)
            .unwrap_or("")
            .into();
        transport.version(protocol.into());
        transport.send(
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            &scope,
        )?;
        if info.pointer("/capabilities/tools").is_none() {
            out.state = "connected".into();
            out.code = "no_tools".into();
            out.message = "握手成功；服务未声明工具能力".into();
            return Ok(());
        }
        out.phase = "tools".into();
        let mut cursor = None;
        let mut cursors = std::collections::BTreeSet::new();
        for page in 0..16u64 {
            let params = cursor
                .as_ref()
                .map(|c| json!({"cursor":c}))
                .unwrap_or(json!({}));
            let list = transport
                .send(
                    &json!({"jsonrpc":"2.0","id":2+page,"method":"tools/list","params":params}),
                    &scope,
                )?
                .ok_or_else(|| fail("protocol_error", "工具列表没有响应"))?;
            let tools = list
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| fail("protocol_error", "tools/list 响应缺少 tools 数组"))?;
            out.tool_count += tools.len();
            for tool in tools {
                if !tool.get("inputSchema").is_some_and(Value::is_object) {
                    return Err(fail(
                        "protocol_error",
                        "工具定义缺少有效的 inputSchema，Agent 可能无法加载",
                    ));
                }
                let name = tool
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| fail("protocol_error", "工具定义缺少名称"))?;
                if out.tools.len() < 200 {
                    out.tools.push(ToolInfo {
                        name: name.to_string(),
                        description: tool
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string(),
                    });
                }
            }
            cursor = list
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if let Some(c) = &cursor {
                if !cursors.insert(c.clone()) {
                    return Err(fail("protocol_error", "工具列表的分页游标重复"));
                }
            } else {
                break;
            }
        }
        out.partial = cursor.is_some();
        out.phase = "complete".into();
        out.state = if out.tool_count > 0 {
            "ok"
        } else {
            "connected"
        }
        .into();
        out.code = if out.tool_count > 0 {
            "available"
        } else {
            "no_tools"
        }
        .into();
        out.message = if out.tool_count > 0 {
            format!(
                "连接正常，已发现 {}{} 个工具",
                out.tool_count,
                if out.partial { "+" } else { "" }
            )
        } else {
            "握手成功，但服务没有返回可调用工具".into()
        };
        Ok(())
    })();
    if let Some(mut transport) = connection {
        transport.close(&scope);
    }
    if let Err(error) = result {
        out.state = match error.code {
            "auth_required" => "auth_required",
            "cancelled" => "cancelled",
            _ => "failed",
        }
        .into();
        out.code = error.code.into();
        out.message = redact(&error.message, &secrets);
        out.details = out.message.clone();
    }
    out.server_name = redact(&out.server_name, &secrets);
    out.server_version = redact(&out.server_version, &secrets);
    for tool in &mut out.tools {
        tool.name = redact(&tool.name, &secrets).chars().take(200).collect();
        tool.description = redact(&tool.description, &secrets)
            .chars()
            .take(240)
            .collect();
    }
    out.elapsed_ms = start.elapsed().as_millis();
    out.checked_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    out
}

#[tauri::command]
pub async fn mcp_probe_check(request: ProbeRequest) -> std::result::Result<ProbeResult, String> {
    if request.id.is_empty() || request.id.len() > 120 {
        return Err("无效的检测标识".into());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut active = ACTIVE.lock().unwrap();
        if active.len() >= 2 {
            return Err("已有检测正在运行，请等待或停止后重试".into());
        }
        if active.contains_key(&request.id) {
            return Err("检测标识重复".into());
        }
        active.insert(request.id.clone(), cancel.clone());
    }
    let id = request.id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || run(&request.server, cancel)).await;
    ACTIVE.lock().unwrap().remove(&id);
    result.map_err(|_| "检测任务异常结束".into())
}
#[tauri::command]
pub fn mcp_probe_cancel(id: String) {
    if let Some(cancel) = ACTIVE.lock().unwrap().get(&id) {
        cancel.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sse_and_rpc_errors_do_not_become_success() {
        assert!(rpc_result(
            json!({"jsonrpc":"2.0","id":1,"error":{"code":-1,"message":"unauthorized"}}),
            1
        )
        .is_err());
        assert!(rpc_result(json!({"id":1,"result":{}}), 1).is_err());
        assert!(rpc_result(json!({"jsonrpc":"2.0","id":2,"result":{}}), 1)
            .unwrap()
            .is_none());
    }
    #[test]
    fn secret_redaction_and_config_quoting() {
        assert!(!redact("failed secret-token", &["secret-token".into()]).contains("secret-token"));
        assert_eq!(quote_config("a\nb\"c"), "\"a\\nb\\\"c\"");
    }
    #[test]
    fn missing_command_and_missing_variable_are_actionable() {
        let server = Server {
            name: "fixture".into(),
            config: json!({"command":"agentskillhub-nonexistent-command-1"}),
            check_timeout_sec: Some(1),
        };
        assert_eq!(
            run(&server, Arc::new(AtomicBool::new(false))).code,
            "missing_command"
        );
        assert!(expand("${input:token}").is_err());
    }
}
