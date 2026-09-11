use crate::contracts::{AppKind, ProviderDraft, RouteState};

/// Paths the caller wants inspected, as display strings.
pub struct DiscoveryPaths {
    pub codex: String,
    pub claude: String,
}

/// What we found for one file.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredFile {
    pub app: AppKind,
    pub path: String,
    pub exists: bool,
    pub state: DiscoveredState,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum DiscoveredState {
    Missing,
    ReadError {
        message: String,
    },
    ParseError {
        message: String,
        line: Option<usize>,
    },
    Ok {
        route: RouteState,
        /// Whether the file already contains app-managed keys.
        managed: bool,
        /// Readiness warnings (unsupported shapes, plaintext secrets…).
        warnings: Vec<String>,
        /// Whether this exact configuration shape can be converted into an
        /// app-owned profile without dropping settings.
        importable: bool,
    },
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryReport {
    pub codex: DiscoveredFile,
    pub claude: DiscoveredFile,
    pub claude_import_proposals: Vec<ClaudeImportProposal>,
}

/// A suggested Claude provider profile derived read-only from discovered
/// content. Codex has a separate complete profile contract.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeImportProposal {
    pub draft: ProviderDraft,
    pub basis: String,
}

impl DiscoveryReport {
    /// The copy persisted in the app-owned discovery cache: identical display
    /// facts with credentials cleared. Import always re-derives the draft from
    /// the live files, so a cached proposal never needs the secret.
    pub fn cached_display(&self) -> DiscoveryReport {
        let mut copy = self.clone();
        for proposal in &mut copy.claude_import_proposals {
            proposal.draft.api_key = String::new();
        }
        if let DiscoveredState::Ok {
            route,
            managed: true,
            ..
        } = &mut copy.codex.state
        {
            route.base_url = route
                .base_url
                .as_ref()
                .map(|_| crate::redact::REDACTED.into());
        }
        copy
    }
}
