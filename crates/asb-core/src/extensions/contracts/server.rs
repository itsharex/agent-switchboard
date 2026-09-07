use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::contracts::AppKind;
use crate::extensions::contracts::SecretValue;

/// Codex-specific options for a server. They have no Claude equivalent and
/// are never translated across clients.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexServerOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub startup_timeout_sec: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_timeout_sec: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

/// The typed MCP server model shared by both clients. Only stdio and HTTP
/// form the commonly editable core; the Claude-only transports are managed
/// read-mostly and never projected to Codex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "transport",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum McpDefinition {
    Stdio {
        command: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<String>,
        /// Environment values handed to the server process.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        env: BTreeMap<String, SecretValue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        codex_options: Option<CodexServerOptions>,
    },
    Http {
        url: String,
        /// Static request headers.
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, SecretValue>,
        /// Bearer credential for the request's Authorization position.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        bearer: Option<SecretValue>,
    },
    /// Deprecated Server-Sent-Events transport, Claude-only. Importable and
    /// deployable to Claude; projection to Codex is refused, not converted.
    ClaudeSse {
        url: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, SecretValue>,
    },
    /// WebSocket transport, Claude-only, same cross-client rule as SSE.
    ClaudeWs {
        url: String,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        headers: BTreeMap<String, SecretValue>,
    },
}

/// One field's explicit treatment in an MCP edit: an absent field is kept
/// verbatim from the current definition (kept values never round-trip
/// through the renderer), a present field is replaced or — only on optional
/// positions — deleted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum FieldEdit<T> {
    Replace { value: T },
    Delete,
}

/// Field-level edits applied against the kept transport. Every position is
/// optional: absent means keep. Entries in `env`/`headers` address one named
/// slot; slots that are not mentioned survive untouched.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpFieldEdits {
    /// stdio command; replace only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<FieldEdit<String>>,
    /// stdio argument list; delete yields the empty list.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<FieldEdit<Vec<String>>>,
    /// stdio environment slots by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, FieldEdit<SecretValue>>,
    /// Remote endpoint URL; replace only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<FieldEdit<String>>,
    /// Static header slots by name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, FieldEdit<SecretValue>>,
    /// HTTP bearer position; delete removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bearer: Option<FieldEdit<SecretValue>>,
    /// Codex-only server options; delete clears them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_options: Option<FieldEdit<CodexServerOptions>>,
}

impl McpFieldEdits {
    pub fn is_empty(&self) -> bool {
        self.command.is_none()
            && self.args.is_none()
            && self.env.is_empty()
            && self.url.is_none()
            && self.headers.is_empty()
            && self.bearer.is_none()
            && self.codex_options.is_none()
    }
}

/// The only accepted shape for editing an existing MCP definition. The
/// request carries the revision it was prepared against; the backend merges
/// it with the persisted definition — kept fields are copied server-side and
/// replaced sensitive values arrive as [`SecretValue::SecretRef`] handles
/// stored beforehand — then validates the synthesized definition as a whole.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpEditRequest {
    /// Definition revision the editor last saw; a mismatch is a conflict.
    pub expected_revision: u64,
    /// New native server key; absent keeps the current one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_key: Option<String>,
    /// Fully specified replacement transport (a switch cannot carry fields
    /// over, so it must be complete); absent keeps the current transport and
    /// applies `fields` to it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<McpDefinition>,
    #[serde(default)]
    pub fields: McpFieldEdits,
}

impl McpDefinition {
    /// The transport label used in list projections and diagnostics.
    pub fn transport_label(&self) -> &'static str {
        match self {
            McpDefinition::Stdio { .. } => "stdio",
            McpDefinition::Http { .. } => "HTTP",
            McpDefinition::ClaudeSse { .. } => "SSE（仅 Claude）",
            McpDefinition::ClaudeWs { .. } => "WebSocket（仅 Claude）",
        }
    }

    /// Whether this definition may be projected to the given client at all.
    pub fn supports_client(&self, client: AppKind) -> bool {
        match self {
            McpDefinition::Stdio { .. } | McpDefinition::Http { .. } => true,
            McpDefinition::ClaudeSse { .. } | McpDefinition::ClaudeWs { .. } => {
                client == AppKind::Claude
            }
        }
    }
}
