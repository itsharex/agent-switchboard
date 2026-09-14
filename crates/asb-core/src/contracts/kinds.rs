use serde::{Deserialize, Serialize};

/// The two supported coding clients.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppKind {
    Codex,
    Claude,
}

impl AppKind {
    /// The display name shared by the interface, tray, and diagnostics.
    pub fn label(self) -> &'static str {
        match self {
            AppKind::Codex => "Codex",
            AppKind::Claude => "Claude Code",
        }
    }

    pub fn config_label(self) -> &'static str {
        match self {
            AppKind::Codex => "~/.codex/config.toml",
            AppKind::Claude => "~/.claude/settings.json",
        }
    }

    /// Directory segment owning this client's persisted application files.
    pub fn dir_name(self) -> &'static str {
        match self {
            AppKind::Codex => "codex",
            AppKind::Claude => "claude",
        }
    }

    /// The one user-global instruction document supported for this client.
    pub fn global_prompt_file_name(self) -> &'static str {
        match self {
            AppKind::Codex => "AGENTS.md",
            AppKind::Claude => "CLAUDE.md",
        }
    }
}

/// One user-global instruction document. The backend owns its absolute target
/// path; the renderer receives only the stable file name, text, and version
/// hash needed to edit it without constructing a filesystem path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GlobalPromptDocument {
    pub app: AppKind,
    pub file_name: String,
    pub content: String,
    pub content_hash: String,
    pub exists: bool,
}

/// How a profile routes a client. The mode is explicit; an empty base URL
/// never implies official routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RouteMode {
    /// Use the client's official login and default endpoint.
    Official,
    /// Route through a user-declared service endpoint.
    Custom,
}

/// The wire protocol spoken by a custom provider's upstream endpoint. This is
/// deliberately the provider's protocol, not the protocol emitted by Codex or
/// Claude Code. The gateway owns conversion whenever the two differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UpstreamProtocol {
    /// OpenAI Responses (`/responses` beneath the declared API root).
    Responses,
    /// OpenAI-compatible Chat Completions (`/chat/completions` beneath the API root).
    ChatCompletions,
    /// Anthropic Messages (`/v1/messages`).
    AnthropicMessages,
    /// Google generateContent, available only through the Claude bridge.
    GeminiGenerateContent,
}

impl UpstreamProtocol {
    /// The only protocol a client can use without this application's local
    /// conversion gateway.
    pub fn native_for(app: AppKind) -> Self {
        match app {
            AppKind::Codex => Self::Responses,
            AppKind::Claude => Self::AnthropicMessages,
        }
    }

    /// Default credential delivery when a provider does not select an override.
    pub fn authentication_scheme(self) -> AuthenticationScheme {
        match self {
            Self::AnthropicMessages => AuthenticationScheme::XApiKey,
            Self::GeminiGenerateContent => AuthenticationScheme::XGoogApiKey,
            Self::Responses | Self::ChatCompletions => AuthenticationScheme::Bearer,
        }
    }

    pub fn resolve_authentication(
        self,
        selected: Option<AuthenticationScheme>,
    ) -> AuthenticationScheme {
        selected.unwrap_or_else(|| self.authentication_scheme())
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Responses => "OpenAI Responses",
            Self::ChatCompletions => "Chat Completions",
            Self::AnthropicMessages => "Anthropic Messages",
            Self::GeminiGenerateContent => "Gemini Native",
        }
    }
}

/// Upstream credential delivery, independent from the body protocol.
/// The local gateway keeps its separate Bearer capability-token contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AuthenticationScheme {
    /// `Authorization: Bearer <key>`.
    Bearer,
    /// `x-api-key: <key>`.
    XApiKey,
    /// `x-goog-api-key: <key>`.
    XGoogApiKey,
}
