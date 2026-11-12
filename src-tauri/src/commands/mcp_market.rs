use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;

#[derive(Clone, Serialize, Deserialize)]
pub struct Source {
    pub id: String,
    pub name: String,
    pub url: String,
    pub kind: String,
}

pub fn sources(custom: &[Source]) -> Vec<Source> {
    let mut out = vec![Source {
        id: "official".into(),
        name: "MCP 官方 Registry".into(),
        url: "https://registry.modelcontextprotocol.io/v0.1/servers".into(),
        kind: "registry".into(),
    }];
    out.extend(custom.iter().filter(|s| s.id != "official").cloned());
    out
}

fn endpoint(source: &Source) -> Result<Url, String> {
    let mut url = Url::parse(source.url.trim()).map_err(|_| "请输入完整的 HTTP(S) 源地址")?;
    if !["http", "https"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("市场源仅支持无账号密码、无锚点的 HTTP(S) 地址".into());
    }
    if source.kind == "registry" && (url.path().is_empty() || url.path() == "/") {
        url.set_path("/v0.1/servers");
    }
    Ok(url)
}

pub fn validate_sources(mut sources: Vec<Source>) -> Result<Vec<Source>, String> {
    sources.retain(|s| s.id != "official");
    if sources.len() > 20 {
        return Err("最多保存 20 个自定义 MCP 源".into());
    }
    let mut ids = std::collections::HashSet::new();
    for s in &mut sources {
        s.name = s.name.trim().into();
        if s.id.is_empty()
            || s.id.len() > 80
            || !s
                .id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_".contains(&b))
            || !ids.insert(s.id.clone())
            || s.name.is_empty()
            || s.name.len() > 200
            || s.url.len() > 2048
            || !["registry", "json"].contains(&s.kind.as_str())
        {
            return Err("市场源名称、标识或格式无效".into());
        }
        s.url = endpoint(s)?.to_string();
    }
    Ok(sources)
}

pub fn normalize(value: Value, kind: &str, query: &str) -> Result<Value, String> {
    if kind == "registry" {
        if !value.get("servers").is_some_and(Value::is_array) {
            return Err("Registry 响应缺少 servers 数组".into());
        }
        return Ok(value);
    }
    let mut rows = if let Some(servers) = value.get("mcpServers").and_then(Value::as_object) {
        servers.iter().map(|(name,config)|json!({"server":{"name":name,"title":name,"description":"自定义 MCP 配置","version":"自定义","config":config}})).collect::<Vec<_>>()
    } else {
        value
            .get("servers")
            .or_else(|| value.as_array().map(|_| &value))
            .and_then(Value::as_array)
            .ok_or("JSON 清单需要 servers 数组或 mcpServers 对象")?
            .iter()
            .map(|row| {
                if row.get("server").is_some() {
                    row.clone()
                } else {
                    json!({"server":row})
                }
            })
            .collect()
    };
    if rows.iter().any(|r| !r["server"]["name"].is_string()) {
        return Err("清单中的每项服务都必须有 name".into());
    }
    let query = query.trim().to_lowercase();
    rows.retain(|r| {
        ["name", "title", "description"].iter().any(|key| {
            r["server"][key]
                .as_str()
                .unwrap_or_default()
                .to_lowercase()
                .contains(&query)
        })
    });
    Ok(json!({"servers":rows,"metadata":{}}))
}

pub fn search(source: &Source, query: &str, cursor: Option<&str>) -> Result<Value, String> {
    let mut url = endpoint(source)?;
    if source.kind == "registry" {
        url.query_pairs_mut()
            .append_pair("version", "latest")
            .append_pair("limit", "30")
            .append_pair("search", query.trim());
        if let Some(cursor) = cursor.filter(|s| !s.is_empty()) {
            url.query_pairs_mut().append_pair("cursor", cursor);
        }
    }
    let out = super::super::hidden_command("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https,http",
            "--proto-redir",
            "=https,http",
            "--connect-timeout",
            "10",
            "--max-time",
            "25",
            "--max-filesize",
            "4194304",
            url.as_str(),
        ])
        .output()
        .map_err(|e| format!("无法连接 MCP 源：{e}"))?;
    if !out.status.success() {
        return Err(format!(
            "无法访问 MCP 源「{}」，请检查源地址和网络后重试",
            source.name
        ));
    }
    let value = serde_json::from_slice(&out.stdout).map_err(|_| "MCP 源返回的内容不是有效 JSON")?;
    normalize(value, &source.kind, query)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn custom_sources_validate_and_keep_schema_explicit() {
        let s = Source {
            id: "mine".into(),
            name: "Mine".into(),
            url: "https://example.org".into(),
            kind: "registry".into(),
        };
        assert_eq!(
            validate_sources(vec![s.clone()]).unwrap()[0].url,
            "https://example.org/v0.1/servers"
        );
        assert!(validate_sources(vec![s.clone(), s.clone()]).is_err());
        assert!(validate_sources(vec![Source {
            url: "file:///etc/hosts".into(),
            ..s
        }])
        .is_err());
    }
    #[test]
    fn json_sources_accept_registry_listings_and_user_configs() {
        let value=normalize(json!({"mcpServers":{"local":{"command":"do-not-run"},"remote":{"url":"https://example.org/mcp"}}}),"json","remote").unwrap();
        assert_eq!(value["servers"].as_array().unwrap().len(), 1);
        assert_eq!(
            value["servers"][0]["server"]["config"]["url"],
            "https://example.org/mcp"
        );
        assert_eq!(normalize(json!({"servers":[{"name":"foo","icons":[{"src":"https://example.org/icon.png"}]}]}),"json","").unwrap()["servers"][0]["server"]["icons"][0]["src"],"https://example.org/icon.png");
        assert!(normalize(json!({"wrong":[]}), "json", "").is_err());
    }
}
