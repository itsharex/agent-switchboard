use std::collections::BTreeMap;

use asb_core::extensions::contracts::McpDefinition;
use asb_core::redact::REDACTED;
use serde::Serialize;

use super::{secret_value_views, SecretValueViewDto};

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpConnectionView {
    transport: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    argument_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env: Option<BTreeMap<String, SecretValueViewDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    headers: Option<BTreeMap<String, SecretValueViewDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bearer: Option<SecretValueViewDto>,
}

impl From<McpDefinition> for McpConnectionView {
    fn from(mcp: McpDefinition) -> Self {
        let transport = match &mcp {
            McpDefinition::Stdio { .. } => "stdio",
            McpDefinition::Http { .. } => "http",
            McpDefinition::ClaudeSse { .. } => "claudeSse",
            McpDefinition::ClaudeWs { .. } => "claudeWs",
        }
        .to_string();
        match mcp {
            McpDefinition::Stdio {
                command, args, env, ..
            } => Self {
                transport,
                command: Some(command),
                argument_count: Some(args.len()),
                env: Some(secret_value_views(env)),
                ..Self::default()
            },
            McpDefinition::Http {
                headers, bearer, ..
            } => Self {
                transport,
                url: Some(REDACTED.into()),
                headers: Some(secret_value_views(headers)),
                bearer: bearer.map(Into::into),
                ..Self::default()
            },
            McpDefinition::ClaudeSse { headers, .. } | McpDefinition::ClaudeWs { headers, .. } => {
                Self {
                    transport,
                    url: Some(REDACTED.into()),
                    headers: Some(secret_value_views(headers)),
                    ..Self::default()
                }
            }
        }
    }
}
