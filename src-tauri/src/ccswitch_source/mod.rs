//! Read-only access to the external provider SQLite database.
//!
//! The database is opened with `mode=ro&immutable=1` so a running source
//! instance is never locked or written. Only the `providers` table is read;
//! custom-provider credentials stay inside the backend mapping for profile
//! import, while scan diagnostics expose only field names.

mod db;

#[cfg(test)]
mod tests;

use crate::local_state::LocalState;
use asb_core::ccswitch;
use asb_core::contracts::AppKind;
use db::{db_path, scan_db};
use serde::Serialize;
use std::path::Path;

/// One importable provider summary with the store's duplicate marking applied.
///
/// This is the complete scan response contract. It deliberately contains only
/// the fields displayed in the renderer and cannot carry an API key.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchScanItem {
    pub key: String,
    pub app: AppKind,
    pub route_mode: asb_core::RouteMode,
    pub name: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    /// Whether the selected import will persist one native usage query. The
    /// source itself remains backend-only.
    pub usage_script_importable: bool,
    /// A routing-identical local profile has no query yet, so this import
    /// enriches that profile instead of creating a duplicate.
    pub usage_script_updates_existing: bool,
    pub warnings: Vec<String>,
    /// An exactly equal profile already exists; importing is a no-op.
    pub existing: bool,
}

/// Full scan result for the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchScan {
    pub db_path: String,
    pub providers: Vec<CcSwitchScanItem>,
    pub skipped: Vec<ccswitch::CcSwitchSkip>,
}

/// Outcome of a batch import.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcSwitchImportOutcome {
    pub imported_count: usize,
    pub usage_script_imported_count: usize,
    pub skipped_existing: Vec<String>,
    pub not_imported: Vec<ccswitch::CcSwitchSkip>,
}

/// Scans the real user database and marks store duplicates.
pub fn scan(state: &LocalState) -> Result<CcSwitchScan, String> {
    scan_at(&db_path()?, state)
}

/// Scans an explicit database path (test entry point; still strictly
/// read-only).
pub fn scan_at(path: &Path, state: &LocalState) -> Result<CcSwitchScan, String> {
    let raw = scan_db(path)?;
    let providers = raw
        .proposals
        .into_iter()
        .map(|proposal| {
            let ccswitch::CcSwitchProposal {
                key,
                app,
                draft,
                warnings,
            } = proposal;
            let existing = state.configuration().provider_exists(&draft);
            let usage_script_importable = draft.usage_query.is_some();
            let usage_script_updates_existing = !existing
                && state
                    .configuration()
                    .provider_will_receive_usage_query(&draft);
            CcSwitchScanItem {
                key,
                app,
                route_mode: draft.route_mode,
                name: draft.name,
                model: draft.model,
                base_url: draft.base_url,
                usage_script_importable,
                usage_script_updates_existing,
                warnings,
                existing,
            }
        })
        .collect();
    Ok(CcSwitchScan {
        db_path: path.to_string_lossy().into_owned(),
        providers,
        skipped: raw.skipped,
    })
}

/// Re-scans the real user database (so a stale preview can never import) and
/// imports the requested keys. Writes only the app's own profile store.
pub fn import(state: &LocalState, keys: &[String]) -> Result<CcSwitchImportOutcome, String> {
    import_at(&db_path()?, state, keys)
}

/// Imports requested keys from an explicit database path (test entry point).
pub fn import_at(
    path: &Path,
    state: &LocalState,
    keys: &[String],
) -> Result<CcSwitchImportOutcome, String> {
    let raw = scan_db(path)?;
    let mut outcome = CcSwitchImportOutcome {
        imported_count: 0,
        usage_script_imported_count: 0,
        skipped_existing: Vec::new(),
        not_imported: Vec::new(),
    };
    for key in keys {
        let Some(proposal) = raw.proposals.iter().find(|p| &p.key == key) else {
            let (name, reason) = match raw.skipped.iter().find(|s| &s.key == key) {
                Some(skip) => (skip.name.clone(), skip.reason.clone()),
                None => (key.clone(), "扫描结果已变化,请重新扫描".to_string()),
            };
            outcome.not_imported.push(ccswitch::CcSwitchSkip {
                key: key.clone(),
                app_type: key.split(':').next().unwrap_or("").to_string(),
                name,
                reason,
            });
            continue;
        };
        if state.configuration().provider_exists(&proposal.draft) {
            outcome.skipped_existing.push(proposal.draft.name.clone());
            continue;
        }
        state
            .configuration()
            .import_provider(proposal.draft.clone())?;
        outcome.imported_count += 1;
        if proposal.draft.usage_query.is_some() {
            outcome.usage_script_imported_count += 1;
        }
    }
    Ok(outcome)
}
