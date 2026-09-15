use std::collections::BTreeMap;

use serde_json::{Map as JsonMap, Value as JsonValue};
use toml_edit::{Item, Table};

use crate::extensions::contracts::{CodexServerOptions, McpDefinition, SecretValue};
use crate::extensions::mcp::observe::{CLAUDE_REMOTE_FIELDS, CLAUDE_STDIO_FIELDS};

/// A native server cannot be represented by the current shared definition.
/// Import is deliberately strict: callers either receive a complete typed
/// definition or an explanation that no write was made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NativeMcpImportError {
    #[error("原生 MCP 配置无法解析")]
    DocumentInvalid,
    #[error("未找到该原生 MCP 服务")]
    MissingServer,
    #[error("该服务的传输类型无法无损导入")]
    UnsupportedTransport,
    #[error("该服务包含当前扩展库无法表达的字段，不能无损导入")]
    UnsupportedFields,
    #[error("该服务的 {0} 字段格式无效，不能无损导入")]
    InvalidField(&'static str),
    #[error("该服务的同一设置出现互斥值，不能无损导入")]
    ConflictingValues,
}

/// Imports one Codex native server into the shared MCP definition model.
///
/// The import accepts only the complete current Codex shape. It deliberately
/// refuses unmodeled or cross-transport fields rather than dropping them.
/// The native enabled flag is verified for type correctness but is not
/// copied: this operation adds a reusable library definition and never takes
/// ownership of the existing client installation.
pub fn import_codex_server(
    document: &str,
    key: &str,
) -> Result<McpDefinition, NativeMcpImportError> {
    let doc = crate::adapter::codex::parse(document)
        .map_err(|_| NativeMcpImportError::DocumentInvalid)?;
    let servers = doc
        .get("mcp_servers")
        .and_then(Item::as_table)
        .ok_or(NativeMcpImportError::MissingServer)?;
    let server = servers
        .get(key)
        .and_then(Item::as_table)
        .ok_or(NativeMcpImportError::MissingServer)?;

    let command = toml_optional_string(server, "command")?;
    let url = toml_optional_string(server, "url")?;
    match (command, url) {
        (Some(command), None) => import_codex_stdio(server, command),
        (None, Some(url)) => import_codex_http(server, url),
        _ => Err(NativeMcpImportError::UnsupportedTransport),
    }
}

fn import_codex_stdio(
    server: &Table,
    command: String,
) -> Result<McpDefinition, NativeMcpImportError> {
    const ALLOWED: &[&str] = &[
        "command",
        "args",
        "env",
        "env_vars",
        "enabled",
        "cwd",
        "startup_timeout_sec",
        "tool_timeout_sec",
        "required",
    ];
    reject_toml_unknown_fields(server, ALLOWED)?;
    validate_toml_optional_bool(server, "enabled")?;

    let mut env = toml_string_map(server, "env")?;
    for variable in toml_string_array(server, "env_vars")? {
        if env
            .insert(variable.clone(), SecretValue::EnvRef { name: variable })
            .is_some()
        {
            return Err(NativeMcpImportError::ConflictingValues);
        }
    }

    let options = CodexServerOptions {
        cwd: toml_optional_string(server, "cwd")?,
        startup_timeout_sec: toml_optional_u64(server, "startup_timeout_sec")?,
        tool_timeout_sec: toml_optional_u64(server, "tool_timeout_sec")?,
        required: toml_optional_bool(server, "required")?,
    };
    Ok(McpDefinition::Stdio {
        command,
        args: toml_string_array(server, "args")?,
        env,
        codex_options: if options == CodexServerOptions::default() {
            None
        } else {
            Some(options)
        },
    })
}

fn import_codex_http(server: &Table, url: String) -> Result<McpDefinition, NativeMcpImportError> {
    const ALLOWED: &[&str] = &[
        "url",
        "bearer_token_env_var",
        "http_headers",
        "env_http_headers",
        "enabled",
    ];
    reject_toml_unknown_fields(server, ALLOWED)?;
    validate_toml_optional_bool(server, "enabled")?;

    let mut headers = toml_string_map(server, "http_headers")?;
    for (name, variable) in toml_env_reference_map(server, "env_http_headers")? {
        if headers.insert(name, variable).is_some() {
            return Err(NativeMcpImportError::ConflictingValues);
        }
    }
    let bearer = toml_optional_string(server, "bearer_token_env_var")?
        .map(|name| SecretValue::EnvRef { name });
    Ok(McpDefinition::Http {
        url,
        headers,
        bearer,
    })
}

/// Imports one Claude native server from a top-level mcpServers object.
/// Claude environment-reference values become environment references; every
/// other literal remains a plain value and is subsequently checked by the
/// common secret and definition validation boundary.
pub fn import_claude_server(
    document: &str,
    key: &str,
) -> Result<McpDefinition, NativeMcpImportError> {
    let root = crate::adapter::claude::parse(document)
        .map_err(|_| NativeMcpImportError::DocumentInvalid)?;
    let servers = root
        .get("mcpServers")
        .and_then(JsonValue::as_object)
        .ok_or(NativeMcpImportError::MissingServer)?;
    import_claude_server_from_map(servers, key)
}

/// Imports one Claude project-private server from
/// projects.<absolute-project-path>.mcpServers.
pub fn import_claude_project_private_server(
    document: &str,
    project_path: &str,
    key: &str,
) -> Result<McpDefinition, NativeMcpImportError> {
    let root = crate::adapter::claude::parse(document)
        .map_err(|_| NativeMcpImportError::DocumentInvalid)?;
    let servers = root
        .get("projects")
        .and_then(JsonValue::as_object)
        .and_then(|projects| projects.get(project_path))
        .and_then(JsonValue::as_object)
        .and_then(|project| project.get("mcpServers"))
        .and_then(JsonValue::as_object)
        .ok_or(NativeMcpImportError::MissingServer)?;
    import_claude_server_from_map(servers, key)
}

fn import_claude_server_from_map(
    servers: &JsonMap<String, JsonValue>,
    key: &str,
) -> Result<McpDefinition, NativeMcpImportError> {
    let server = servers
        .get(key)
        .and_then(JsonValue::as_object)
        .ok_or(NativeMcpImportError::MissingServer)?;
    let transport = json_optional_string(server, "type")?;
    match transport.as_deref() {
        Some("stdio") | None if server.contains_key("command") => import_claude_stdio(server),
        Some("http") => import_claude_remote(server, "http"),
        Some("sse") => import_claude_remote(server, "sse"),
        Some("ws") => import_claude_remote(server, "ws"),
        _ => Err(NativeMcpImportError::UnsupportedTransport),
    }
}

fn import_claude_stdio(
    server: &JsonMap<String, JsonValue>,
) -> Result<McpDefinition, NativeMcpImportError> {
    reject_json_unknown_fields(server, CLAUDE_STDIO_FIELDS)?;
    let command = json_required_string(server, "command")?;
    let args = json_string_array(server, "args")?;
    // A Windows `cmd /c npx …` entry is the host form of the portable
    // launcher; the library keeps the portable form and re-wraps on render.
    let (command, args) =
        super::unwrap_windows_launcher(&command, &args).unwrap_or((command, args));
    Ok(McpDefinition::Stdio {
        command,
        args,
        env: json_secret_map(server, "env")?,
        codex_options: None,
    })
}

fn import_claude_remote(
    server: &JsonMap<String, JsonValue>,
    transport: &str,
) -> Result<McpDefinition, NativeMcpImportError> {
    reject_json_unknown_fields(server, CLAUDE_REMOTE_FIELDS)?;
    let url = json_required_string(server, "url")?;
    let headers = json_secret_map(server, "headers")?;
    match transport {
        "http" => Ok(McpDefinition::Http {
            url,
            headers,
            bearer: None,
        }),
        "sse" => Ok(McpDefinition::ClaudeSse { url, headers }),
        "ws" => Ok(McpDefinition::ClaudeWs { url, headers }),
        _ => Err(NativeMcpImportError::UnsupportedTransport),
    }
}

fn reject_toml_unknown_fields(table: &Table, allowed: &[&str]) -> Result<(), NativeMcpImportError> {
    if table.iter().any(|(name, _)| !allowed.contains(&name)) {
        return Err(NativeMcpImportError::UnsupportedFields);
    }
    Ok(())
}

fn toml_optional_string(
    table: &Table,
    field: &'static str,
) -> Result<Option<String>, NativeMcpImportError> {
    match table.get(field) {
        None => Ok(None),
        Some(item) => item
            .as_str()
            .map(|value| Some(value.to_string()))
            .ok_or(NativeMcpImportError::InvalidField(field)),
    }
}

fn toml_optional_bool(
    table: &Table,
    field: &'static str,
) -> Result<Option<bool>, NativeMcpImportError> {
    match table.get(field) {
        None => Ok(None),
        Some(item) => item
            .as_bool()
            .map(Some)
            .ok_or(NativeMcpImportError::InvalidField(field)),
    }
}

fn validate_toml_optional_bool(
    table: &Table,
    field: &'static str,
) -> Result<(), NativeMcpImportError> {
    toml_optional_bool(table, field).map(|_| ())
}

fn toml_optional_u64(
    table: &Table,
    field: &'static str,
) -> Result<Option<u64>, NativeMcpImportError> {
    match table.get(field) {
        None => Ok(None),
        Some(item) => item
            .as_integer()
            .and_then(|value| u64::try_from(value).ok())
            .map(Some)
            .ok_or(NativeMcpImportError::InvalidField(field)),
    }
}

fn toml_string_array(
    table: &Table,
    field: &'static str,
) -> Result<Vec<String>, NativeMcpImportError> {
    let Some(item) = table.get(field) else {
        return Ok(Vec::new());
    };
    let array = item
        .as_array()
        .ok_or(NativeMcpImportError::InvalidField(field))?;
    array
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or(NativeMcpImportError::InvalidField(field))
        })
        .collect()
}

fn toml_string_map(
    table: &Table,
    field: &'static str,
) -> Result<BTreeMap<String, SecretValue>, NativeMcpImportError> {
    let Some(item) = table.get(field) else {
        return Ok(BTreeMap::new());
    };
    let map = item
        .as_table_like()
        .ok_or(NativeMcpImportError::InvalidField(field))?;
    map.iter()
        .map(|(name, value)| {
            value
                .as_str()
                .map(|value| {
                    (
                        name.to_string(),
                        SecretValue::Plain {
                            value: value.to_string(),
                        },
                    )
                })
                .ok_or(NativeMcpImportError::InvalidField(field))
        })
        .collect()
}

fn toml_env_reference_map(
    table: &Table,
    field: &'static str,
) -> Result<BTreeMap<String, SecretValue>, NativeMcpImportError> {
    let Some(item) = table.get(field) else {
        return Ok(BTreeMap::new());
    };
    let map = item
        .as_table_like()
        .ok_or(NativeMcpImportError::InvalidField(field))?;
    map.iter()
        .map(|(name, value)| {
            value
                .as_str()
                .map(|variable| {
                    (
                        name.to_string(),
                        SecretValue::EnvRef {
                            name: variable.to_string(),
                        },
                    )
                })
                .ok_or(NativeMcpImportError::InvalidField(field))
        })
        .collect()
}

fn reject_json_unknown_fields(
    server: &JsonMap<String, JsonValue>,
    allowed: &[&str],
) -> Result<(), NativeMcpImportError> {
    if server.keys().any(|name| !allowed.contains(&name.as_str())) {
        return Err(NativeMcpImportError::UnsupportedFields);
    }
    Ok(())
}

fn json_optional_string(
    server: &JsonMap<String, JsonValue>,
    field: &'static str,
) -> Result<Option<String>, NativeMcpImportError> {
    match server.get(field) {
        None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|value| Some(value.to_string()))
            .ok_or(NativeMcpImportError::InvalidField(field)),
    }
}

fn json_required_string(
    server: &JsonMap<String, JsonValue>,
    field: &'static str,
) -> Result<String, NativeMcpImportError> {
    json_optional_string(server, field)?.ok_or(NativeMcpImportError::InvalidField(field))
}

fn json_string_array(
    server: &JsonMap<String, JsonValue>,
    field: &'static str,
) -> Result<Vec<String>, NativeMcpImportError> {
    let Some(value) = server.get(field) else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or(NativeMcpImportError::InvalidField(field))?;
    array
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or(NativeMcpImportError::InvalidField(field))
        })
        .collect()
}

fn json_secret_map(
    server: &JsonMap<String, JsonValue>,
    field: &'static str,
) -> Result<BTreeMap<String, SecretValue>, NativeMcpImportError> {
    let Some(value) = server.get(field) else {
        return Ok(BTreeMap::new());
    };
    let map = value
        .as_object()
        .ok_or(NativeMcpImportError::InvalidField(field))?;
    map.iter()
        .map(|(name, value)| {
            let value = value
                .as_str()
                .ok_or(NativeMcpImportError::InvalidField(field))?;
            Ok((name.clone(), native_secret_value(value)))
        })
        .collect()
}

fn native_secret_value(value: &str) -> SecretValue {
    let env_name = value
        .strip_prefix("${")
        .and_then(|value| value.strip_suffix('}'));
    match env_name {
        Some(name) => SecretValue::EnvRef {
            name: name.to_string(),
        },
        None => SecretValue::Plain {
            value: value.to_string(),
        },
    }
}
