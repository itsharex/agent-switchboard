use std::collections::BTreeMap;

use serde::Serialize;

use crate::extensions::contracts::{CodexServerOptions, McpDefinition, SecretValue};

// ---------------------------------------------------------------- editor view

/// One editable secret-bearing position as shown to the editor. It carries
/// the stored kind and every value that is not credential material; a stored
/// secret reference is reduced to a presence marker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "mode", rename_all = "camelCase")]
pub enum SecretSlotView {
    /// Non-sensitive literal (secret-shaped literals never enter the
    /// library, so a stored plain value is displayable by construction).
    Plain { value: String },
    /// Reference to a host environment variable by exact name.
    EnvRef { name: String },
    /// A credential is configured in the system store. Its value never
    /// leaves the backend.
    SecretConfigured,
}

/// One named secret-bearing position in the view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretSlot {
    pub name: String,
    pub value: SecretSlotView,
}

/// The redacted, editable projection of one MCP definition payload. Field
/// coverage matches [`McpDefinition`] exactly, so the editor can prefilled
/// every position the write-side contract can change without ever seeing
/// stored credential material.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "transport", rename_all = "camelCase")]
pub enum McpEditView {
    Stdio {
        command: String,
        args: Vec<String>,
        env: Vec<SecretSlot>,
        codex_options: Option<CodexServerOptions>,
    },
    Http {
        url: String,
        headers: Vec<SecretSlot>,
        bearer: Option<SecretSlotView>,
    },
    ClaudeSse {
        url: String,
        headers: Vec<SecretSlot>,
    },
    ClaudeWs {
        url: String,
        headers: Vec<SecretSlot>,
    },
}

fn slot_view(value: &SecretValue) -> SecretSlotView {
    match value {
        SecretValue::EnvRef { name } => SecretSlotView::EnvRef { name: name.clone() },
        SecretValue::Plain { value } => SecretSlotView::Plain {
            value: value.clone(),
        },
        SecretValue::SecretRef { .. } => SecretSlotView::SecretConfigured,
    }
}

fn slot_list(map: &BTreeMap<String, SecretValue>) -> Vec<SecretSlot> {
    map.iter()
        .map(|(name, value)| SecretSlot {
            name: name.clone(),
            value: slot_view(value),
        })
        .collect()
}

/// Builds the editor projection of one stored definition.
pub fn mcp_edit_view(definition: &McpDefinition) -> McpEditView {
    match definition {
        McpDefinition::Stdio {
            command,
            args,
            env,
            codex_options,
        } => McpEditView::Stdio {
            command: command.clone(),
            args: args.clone(),
            env: slot_list(env),
            codex_options: codex_options.clone(),
        },
        McpDefinition::Http {
            url,
            headers,
            bearer,
        } => McpEditView::Http {
            url: url.clone(),
            headers: slot_list(headers),
            bearer: bearer.as_ref().map(slot_view),
        },
        McpDefinition::ClaudeSse { url, headers } => McpEditView::ClaudeSse {
            url: url.clone(),
            headers: slot_list(headers),
        },
        McpDefinition::ClaudeWs { url, headers } => McpEditView::ClaudeWs {
            url: url.clone(),
            headers: slot_list(headers),
        },
    }
}
