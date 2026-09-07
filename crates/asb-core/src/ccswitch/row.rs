use serde::{Deserialize, Serialize};

use crate::contracts::{AppKind, ProviderDraft};

/// One source `providers` row, already read by the caller.
#[derive(Debug, Clone)]
pub struct CcSwitchRow {
    pub id: String,
    pub app_type: String,
    pub name: String,
    pub settings_config: String,
    /// Local metadata carried over on import; both columns are nullable.
    pub website_url: Option<String>,
    pub notes: Option<String>,
    /// Source-provider metadata. Only the optional usage script is
    /// considered; its source never crosses the scan-response boundary.
    pub meta: Option<String>,
}

/// An importable provider. `key` identifies the source
/// row (`"<app_type>:<id>"`) so the import command can re-resolve it against
/// a fresh scan.
#[derive(Debug, Clone, PartialEq)]
pub struct CcSwitchProposal {
    pub key: String,
    pub app: AppKind,
    pub draft: ProviderDraft,
    /// Field names that exist in the source but are not carried over. Never
    /// contains secret values, only key names.
    pub warnings: Vec<String>,
}

/// A provider that cannot be imported, with a user-facing reason.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchSkip {
    pub key: String,
    pub app_type: String,
    pub name: String,
    pub reason: String,
}
