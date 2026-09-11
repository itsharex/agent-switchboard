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
    /// Extra per-entry notes that are not problems (e.g. a field the client
    /// ignores in this scope).
    pub notes: Vec<String>,
    /// The typed transport problem of this entry, if any.
    pub problem: McpEntryProblem,
}

/// The typed transport problem of one observed entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum McpEntryProblem {
    /// The entry is readable and its transport is decided.
    None,
    /// The entry is not a table/object, so no fields could be read.
    NotAnObject,
    /// The entry carries neither a url nor a command.
    TransportMissing,
    /// The entry carries both a url and a command.
    TransportConflicting,
    /// The entry names a transport type this contract does not know.
    UnknownTransportType { type_name: String },
}

impl McpEntryProblem {
    pub fn is_none(&self) -> bool {
        matches!(self, McpEntryProblem::None)
    }
}

/// The typed problem of the MCP collection itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum McpCollectionProblem {
    /// No collection key, or the collection is empty and well-formed.
    None,
    /// The collection key exists but is not a table/object. The document
    /// must never be reported as having zero servers in this state.
    InvalidType,
}

impl McpCollectionProblem {
    pub fn is_none(&self) -> bool {
        matches!(self, McpCollectionProblem::None)
    }
}

/// Everything one document read observes: its server entries plus the
/// collection-level problem, so callers can report a malformed collection
/// instead of silently reading it as empty.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedMcpDocument {
    pub servers: Vec<ObservedMcpServer>,
    pub collection_problem: McpCollectionProblem,
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

/// Reads the `mcp_servers` collection from a Codex `config.toml`. A present
/// but non-table collection is a typed problem, never an empty list.
pub fn read_codex_servers(
    document: &str,
) -> Result<ObservedMcpDocument, crate::adapter::AdapterError> {
    let doc = crate::adapter::codex::parse(document)?;
    let mut servers = Vec::new();
    let Some(item) = doc.get("mcp_servers") else {
        return Ok(ObservedMcpDocument {
            servers,
            collection_problem: McpCollectionProblem::None,
        });
    };
    let Item::Table(mcp_table) = item else {
        return Ok(ObservedMcpDocument {
            servers,
            collection_problem: McpCollectionProblem::InvalidType,
        });
    };
    for (key, entry) in mcp_table.iter() {
        servers.push(match entry.as_table() {
            Some(table) => read_codex_server_table(key, table),
            None => ObservedMcpServer {
                key: key.to_string(),
                transport: ObservedTransport::Unknown,
                enabled: None,
                unknown_fields: Vec::new(),
                notes: Vec::new(),
                problem: McpEntryProblem::NotAnObject,
            },
        });
    }
    Ok(ObservedMcpDocument {
        servers,
        collection_problem: McpCollectionProblem::None,
    })
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
    let (transport, problem) = match (url, command) {
        (Some(url), None) => (ObservedTransport::Http { url }, McpEntryProblem::None),
        (None, Some(command)) => (ObservedTransport::Stdio { command }, McpEntryProblem::None),
        (Some(_), Some(_)) => (
            ObservedTransport::Unknown,
            McpEntryProblem::TransportConflicting,
        ),
        (None, None) => (
            ObservedTransport::Unknown,
            McpEntryProblem::TransportMissing,
        ),
    };
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
        notes: Vec::new(),
        problem,
    }
}

/// Reads one Claude `mcpServers`-shaped collection located by `read_collection`
/// (root document → collection value, `None` when absent). A present but
/// non-object collection is a typed problem, never an empty list.
pub fn read_claude_servers(
    document: &str,
    read_collection: impl FnOnce(&JsonValue) -> Option<&JsonValue>,
) -> Result<ObservedMcpDocument, crate::adapter::AdapterError> {
    let root = crate::adapter::claude::parse(document)?;
    let mut servers = Vec::new();
    let Some(collection) = read_collection(&root) else {
        return Ok(ObservedMcpDocument {
            servers,
            collection_problem: McpCollectionProblem::None,
        });
    };
    let Some(map) = collection.as_object() else {
        return Ok(ObservedMcpDocument {
            servers,
            collection_problem: McpCollectionProblem::InvalidType,
        });
    };
    for (key, entry) in map {
        servers.push(read_claude_server_entry(key, entry));
    }
    Ok(ObservedMcpDocument {
        servers,
        collection_problem: McpCollectionProblem::None,
    })
}

fn read_claude_server_entry(key: &str, entry: &JsonValue) -> ObservedMcpServer {
    let Some(object) = entry.as_object() else {
        return ObservedMcpServer {
            key: key.to_string(),
            transport: ObservedTransport::Unknown,
            enabled: None,
            unknown_fields: Vec::new(),
            notes: Vec::new(),
            problem: McpEntryProblem::NotAnObject,
        };
    };
    let string_field = |name: &str| {
        object
            .get(name)
            .and_then(|value| value.as_str().map(str::to_string))
    };
    let type_field = string_field("type");
    let (transport, problem) = match type_field.as_deref() {
        Some("stdio") => match string_field("command") {
            Some(command) => (ObservedTransport::Stdio { command }, McpEntryProblem::None),
            None => (
                ObservedTransport::Unknown,
                McpEntryProblem::TransportMissing,
            ),
        },
        Some("http") => match string_field("url") {
            Some(url) => (ObservedTransport::Http { url }, McpEntryProblem::None),
            None => (
                ObservedTransport::Unknown,
                McpEntryProblem::TransportMissing,
            ),
        },
        Some("sse") => match string_field("url") {
            Some(url) => (ObservedTransport::ClaudeSse { url }, McpEntryProblem::None),
            None => (
                ObservedTransport::Unknown,
                McpEntryProblem::TransportMissing,
            ),
        },
        Some("ws") => match string_field("url") {
            Some(url) => (ObservedTransport::ClaudeWs { url }, McpEntryProblem::None),
            None => (
                ObservedTransport::Unknown,
                McpEntryProblem::TransportMissing,
            ),
        },
        // Older documents omit `type` for stdio.
        None => match string_field("command") {
            Some(command) => (ObservedTransport::Stdio { command }, McpEntryProblem::None),
            None => (
                ObservedTransport::Unknown,
                McpEntryProblem::TransportMissing,
            ),
        },
        Some(other) => (
            ObservedTransport::Unknown,
            McpEntryProblem::UnknownTransportType {
                type_name: other.to_string(),
            },
        ),
    };
    let modeled: &[&str] = match transport {
        ObservedTransport::Stdio { .. } => CLAUDE_STDIO_FIELDS,
        ObservedTransport::Http { .. }
        | ObservedTransport::ClaudeSse { .. }
        | ObservedTransport::ClaudeWs { .. } => CLAUDE_REMOTE_FIELDS,
        ObservedTransport::Unknown => &[],
    };
    let mut notes = Vec::new();
    if object.contains_key("enabled") {
        notes.push(
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
        notes,
        problem,
    }
}
