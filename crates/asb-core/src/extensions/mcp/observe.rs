use serde::Serialize;
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml_edit::{Item, Table};

/// One MCP server observed inside a native document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedMcpServer {
    pub key: String,
    pub transport: ObservedTransport,
    /// Codex-native `enabled`; Claude has no per-server enabled field.
    pub enabled: Option<bool>,
    /// Field names present in the native entry that this contract does not
    /// model. They stay untouched in the document.
    pub unknown_fields: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ObservedTransport {
    Stdio { command: String },
    Http { url: String },
    ClaudeSse { url: String },
    ClaudeWs { url: String },
    Unknown,
}

impl ObservedTransport {
    pub fn label(&self) -> &'static str {
        match self {
            ObservedTransport::Stdio { .. } => "stdio",
            ObservedTransport::Http { .. } => "HTTP",
            ObservedTransport::ClaudeSse { .. } => "SSE",
            ObservedTransport::ClaudeWs { .. } => "WebSocket",
            ObservedTransport::Unknown => "未知",
        }
    }
}

/// Codex server fields this contract models.
pub(super) const CODEX_MODELED_FIELDS: &[&str] = &[
    "command",
    "args",
    "env",
    "env_vars",
    "url",
    "bearer_token_env_var",
    "http_headers",
    "env_http_headers",
    "enabled",
    "cwd",
    "startup_timeout_sec",
    "tool_timeout_sec",
    "required",
];

/// The stdio fields Claude models; everything else per type is unknown.
pub(super) const CLAUDE_STDIO_FIELDS: &[&str] = &["type", "command", "args", "env"];
pub(super) const CLAUDE_REMOTE_FIELDS: &[&str] = &["type", "url", "headers"];

/// The modeled field set for one existing Claude entry, chosen by its
/// transport shape: unknown transport models only `type`, so every other
/// host field is preserved on patch.
pub(super) fn claude_modeled_fields_for(
    existing: &JsonMap<String, JsonValue>,
) -> &'static [&'static str] {
    match existing.get("type").and_then(|value| value.as_str()) {
        Some("stdio") | None if existing.contains_key("command") => CLAUDE_STDIO_FIELDS,
        Some("http") | Some("sse") | Some("ws") => CLAUDE_REMOTE_FIELDS,
        _ => &["type"],
    }
}

/// Reads the `mcp_servers` tables from a Codex `config.toml`.
pub fn read_codex_servers(
    document: &str,
) -> Result<Vec<ObservedMcpServer>, crate::adapter::AdapterError> {
    let doc = crate::adapter::codex::parse(document)?;
    let mut servers = Vec::new();
    let Some(Item::Table(mcp_table)) = doc.get("mcp_servers") else {
        return Ok(servers);
    };
    for (key, item) in mcp_table.iter() {
        let Some(table) = item.as_table() else {
            servers.push(ObservedMcpServer {
                key: key.to_string(),
                transport: ObservedTransport::Unknown,
                enabled: None,
                unknown_fields: Vec::new(),
                diagnostics: vec!["mcp_servers 的该条目不是表，无法解读".to_string()],
            });
            continue;
        };
        servers.push(read_codex_server_table(key, table));
    }
    Ok(servers)
}

fn read_codex_server_table(key: &str, table: &Table) -> ObservedMcpServer {
    let string_field = |name: &str| {
        table
            .get(name)
            .and_then(|item| item.as_str().map(str::to_string))
    };
    let bool_field = |name: &str| table.get(name).and_then(|item| item.as_bool());
    let url = string_field("url");
    let command = string_field("command");
    let transport = match (url, command) {
        (Some(url), None) => ObservedTransport::Http { url },
        (None, Some(command)) => ObservedTransport::Stdio { command },
        (Some(_), Some(_)) => ObservedTransport::Unknown,
        (None, None) => ObservedTransport::Unknown,
    };
    let mut diagnostics = Vec::new();
    if matches!(transport, ObservedTransport::Unknown) {
        diagnostics.push("该服务缺少 url 或 command，传输类型无法判定".to_string());
    }
    let unknown_fields: Vec<String> = table
        .iter()
        .map(|(name, _)| name.to_string())
        .filter(|name| !CODEX_MODELED_FIELDS.contains(&name.as_str()))
        .collect();
    ObservedMcpServer {
        key: key.to_string(),
        transport,
        enabled: bool_field("enabled"),
        unknown_fields,
        diagnostics,
    }
}

/// Reads one Claude `mcpServers` object located at `pointer_description`
/// (used only for diagnostics). `extract` obtains the object from the parsed
/// document.
pub fn read_claude_servers(
    document: &str,
    read_object: impl FnOnce(&JsonValue) -> Option<&JsonMap<String, JsonValue>>,
) -> Result<Vec<ObservedMcpServer>, crate::adapter::AdapterError> {
    let root = crate::adapter::claude::parse(document)?;
    let mut servers = Vec::new();
    let Some(map) = read_object(&root) else {
        return Ok(servers);
    };
    for (key, entry) in map {
        servers.push(read_claude_server_entry(key, entry));
    }
    Ok(servers)
}

fn read_claude_server_entry(key: &str, entry: &JsonValue) -> ObservedMcpServer {
    let Some(object) = entry.as_object() else {
        return ObservedMcpServer {
            key: key.to_string(),
            transport: ObservedTransport::Unknown,
            enabled: None,
            unknown_fields: Vec::new(),
            diagnostics: vec!["该条目不是对象，无法解读".to_string()],
        };
    };
    let string_field = |name: &str| {
        object
            .get(name)
            .and_then(|value| value.as_str().map(str::to_string))
    };
    let type_field = string_field("type");
    let transport = match type_field.as_deref() {
        Some("stdio") => string_field("command").map_or(ObservedTransport::Unknown, |command| {
            ObservedTransport::Stdio { command }
        }),
        Some("http") => string_field("url").map_or(ObservedTransport::Unknown, |url| {
            ObservedTransport::Http { url }
        }),
        Some("sse") => string_field("url").map_or(ObservedTransport::Unknown, |url| {
            ObservedTransport::ClaudeSse { url }
        }),
        Some("ws") => string_field("url").map_or(ObservedTransport::Unknown, |url| {
            ObservedTransport::ClaudeWs { url }
        }),
        // Older documents omit `type` for stdio.
        None => string_field("command").map_or(ObservedTransport::Unknown, |command| {
            ObservedTransport::Stdio { command }
        }),
        Some(other) => {
            return ObservedMcpServer {
                key: key.to_string(),
                transport: ObservedTransport::Unknown,
                enabled: None,
                unknown_fields: Vec::new(),
                diagnostics: vec![format!("未知传输类型 {other}；按只读展示")],
            };
        }
    };
    let modeled: &[&str] = match transport {
        ObservedTransport::Stdio { .. } => CLAUDE_STDIO_FIELDS,
        ObservedTransport::Http { .. }
        | ObservedTransport::ClaudeSse { .. }
        | ObservedTransport::ClaudeWs { .. } => CLAUDE_REMOTE_FIELDS,
        ObservedTransport::Unknown => &[],
    };
    let mut diagnostics = Vec::new();
    if matches!(transport, ObservedTransport::Unknown) {
        diagnostics.push("该服务缺少 type 与 command，传输类型无法判定".to_string());
    }
    if object.contains_key("enabled") {
        diagnostics.push(
            "Claude 的服务定义没有原生 enabled 字段；停用属于项目级 disabledMcpServers".to_string(),
        );
    }
    let unknown_fields: Vec<String> = object
        .keys()
        .cloned()
        .filter(|name| !modeled.contains(&name.as_str()))
        .collect();
    ObservedMcpServer {
        key: key.to_string(),
        transport,
        enabled: None,
        unknown_fields,
        diagnostics,
    }
}
