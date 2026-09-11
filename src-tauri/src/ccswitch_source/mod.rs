//! Read-only access to the external provider SQLite database.
//!
//! The database is opened with `mode=ro&immutable=1` so a running source
//! instance is never locked or written. Only the `providers` table is read.
//! Claude credentials stay inside the backend until the batch import writes
//! profiles; a Codex credential only leaves through the single-row
//! `prepare_codex_seed` completion command, and scan diagnostics expose
//! field names only.

mod db;

#[cfg(test)]
mod tests;

use crate::local_state::LocalState;
use asb_core::ccswitch;
use asb_core::contracts::{AppKind, ProviderDraft, RouteMode};
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
        .map(|proposal| scan_item(state, proposal))
        .collect();
    Ok(CcSwitchScan {
        db_path: path.to_string_lossy().into_owned(),
        providers,
        skipped: raw.skipped,
    })
}

/// Re-scans the real user database (so a stale preview can never import) and
/// imports the requested Claude keys. Writes only the app's own profile
/// store. Codex rows are completed in the editor instead: passing one to
/// this command is a caller bug and fails before any write.
pub fn import(state: &LocalState, keys: &[String]) -> Result<CcSwitchImportOutcome, String> {
    import_at(&db_path()?, state, keys)
}

/// Imports requested Claude keys from an explicit database path (test entry
/// point).
pub fn import_at(
    path: &Path,
    state: &LocalState,
    keys: &[String],
) -> Result<CcSwitchImportOutcome, String> {
    let raw = scan_db(path)?;
    if raw.proposals.iter().any(|proposal| {
        keys.contains(&proposal.key)
            && matches!(proposal.draft, ccswitch::CcSwitchProviderDraft::Codex(_))
    }) {
        return Err(
            "Codex 供应商必须逐项补全导入；请在扫描列表中选择该供应商的「补全导入」".to_string(),
        );
    }
    let claude_proposals: Vec<(&str, &ProviderDraft)> = raw
        .proposals
        .iter()
        .filter_map(|proposal| match &proposal.draft {
            ccswitch::CcSwitchProviderDraft::Claude(draft) => Some((proposal.key.as_str(), draft)),
            ccswitch::CcSwitchProviderDraft::Codex(_) => None,
        })
        .collect();
    let mut outcome = CcSwitchImportOutcome {
        imported_count: 0,
        usage_script_imported_count: 0,
        skipped_existing: Vec::new(),
        not_imported: Vec::new(),
    };
    for key in keys {
        let Some(draft) = claude_proposals
            .iter()
            .find(|(proposal_key, _)| proposal_key == &key.as_str())
            .map(|(_, draft)| *draft)
        else {
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
        if state.configuration().provider_exists(draft) {
            outcome.skipped_existing.push(draft.name.clone());
            continue;
        }
        state.configuration().import_provider(draft.clone())?;
        if draft.usage_query.is_some() {
            outcome.usage_script_imported_count += 1;
        }
        outcome.imported_count += 1;
    }
    Ok(outcome)
}

/// Re-scans the real user database and returns the completion seed for one
/// Codex row. This is the only boundary where a source credential crosses to
/// the renderer — one deliberately chosen row at a time, never the whole
/// scan — and nothing is persisted here; the editor saves through the normal
/// create command after the user confirms the catalog and capabilities.
pub fn prepare_codex_seed(key: &str) -> Result<ccswitch::CodexImportSeed, String> {
    prepare_codex_seed_at(&db_path()?, key)
}

/// Returns the completion seed for one Codex row from an explicit database
/// path (test entry point; still strictly read-only).
pub fn prepare_codex_seed_at(path: &Path, key: &str) -> Result<ccswitch::CodexImportSeed, String> {
    let raw = scan_db(path)?;
    let Some(proposal) = raw
        .proposals
        .into_iter()
        .find(|proposal| proposal.key == key)
    else {
        return Err(format!("扫描结果已变化，请重新扫描: {key}"));
    };
    match proposal.draft {
        ccswitch::CcSwitchProviderDraft::Codex(seed) => Ok(seed),
        ccswitch::CcSwitchProviderDraft::Claude(_) => Err(format!("{key} 不是 Codex 供应商")),
    }
}

fn scan_item(state: &LocalState, proposal: ccswitch::CcSwitchProposal) -> CcSwitchScanItem {
    let ccswitch::CcSwitchProposal {
        key,
        draft,
        warnings,
    } = proposal;
    match draft {
        ccswitch::CcSwitchProviderDraft::Claude(draft) => {
            let existing = state.configuration().provider_exists(&draft);
            let usage_script_updates_existing = !existing
                && state
                    .configuration()
                    .provider_will_receive_usage_query(&draft);
            CcSwitchScanItem {
                key,
                app: AppKind::Claude,
                route_mode: draft.route_mode,
                name: draft.name,
                model: draft.model,
                base_url: draft.base_url,
                usage_script_importable: draft.usage_query.is_some(),
                usage_script_updates_existing,
                warnings,
                existing,
            }
        }
        ccswitch::CcSwitchProviderDraft::Codex(seed) => {
            let existing = state
                .configuration()
                .codex_route_exists(&seed.endpoint, seed.upstream);
            CcSwitchScanItem {
                key,
                app: AppKind::Codex,
                route_mode: RouteMode::Custom,
                name: seed.name,
                model: Some(seed.default_model),
                base_url: Some(seed.endpoint.0),
                usage_script_importable: seed.usage_query.is_some(),
                // Codex rows are completed in the editor; there is no batch
                // enrichment path that could update an existing profile.
                usage_script_updates_existing: false,
                warnings: seed.warnings,
                existing,
            }
        }
    }
}
