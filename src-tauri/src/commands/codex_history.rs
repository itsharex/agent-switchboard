//! Codex 跨供应商统一历史迁移命令（S03）：只读桶扫描、带账本的存量迁移、
//! 以及按账本精确还原。所有写入都在用户真实的 `~/.codex` 历史数据上，因此
//! 每条变更命令都先备份原对象；真实数据验收需要用户明确触发。

use super::error::{blocking, state, CommandError};
use crate::codex_history_unify::{self, BucketScan, RestoreOutcome, UnifyOutcome};
use crate::local_state::LocalState;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexHistoryBuckets {
    pub(crate) migration_completed: bool,
    pub(crate) backup_available: bool,
    pub(crate) buckets: Vec<BucketScan>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexHistoryMigrateOutcome {
    pub(crate) legacy_default: bool,
    #[serde(flatten)]
    pub(crate) outcome: UnifyOutcome,
}

fn store_error(error: String) -> CommandError {
    CommandError::new("codex-history-unify", error)
}

fn codex_root(_state: &LocalState) -> Result<std::path::PathBuf, CommandError> {
    let path = LocalState::codex_auth_path()
        .map_err(|error| CommandError::new("codex-root-unavailable", error))?;
    Ok(path.parent().map(PathBuf::from).unwrap_or_default())
}

fn config_text(state: &LocalState) -> String {
    state
        .target(asb_core::contracts::AppKind::Codex)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .unwrap_or_default()
}

#[tauri::command]
pub(crate) async fn scan_codex_history_buckets(
    app: AppHandle,
) -> Result<CodexHistoryBuckets, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let codex_root = codex_root(&state)?;
        let config = config_text(&state);
        let buckets =
            codex_history_unify::scan_buckets(&codex_root, &config).map_err(store_error)?;
        Ok(CodexHistoryBuckets {
            migration_completed: codex_history_unify::migration_completed(
                &state.root(),
                &codex_root,
            ),
            backup_available: codex_history_unify::has_backup(&state.root()),
            buckets,
        })
    })
    .await
}

/// Migrates the given legacy buckets (or the known legacy preset ids when
/// omitted) into the unified `openai` bucket.
#[tauri::command]
pub(crate) async fn migrate_codex_history_to_unified(
    app: AppHandle,
    source_provider_ids: Option<Vec<String>>,
) -> Result<CodexHistoryMigrateOutcome, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let codex_root = codex_root(&state)?;
        let config = config_text(&state);
        let legacy_default = source_provider_ids.is_none();
        let sources: Vec<String> = source_provider_ids.unwrap_or_else(|| {
            codex_history_unify::LEGACY_SOURCE_PROVIDER_IDS
                .iter()
                .map(|id| id.to_string())
                .collect()
        });
        let outcome = codex_history_unify::migrate_to_unified(
            &state.root(),
            &codex_root,
            &config,
            &sources.into_iter().collect::<BTreeSet<String>>(),
        )
        .map_err(store_error)?;
        Ok(CodexHistoryMigrateOutcome {
            legacy_default,
            outcome,
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn has_codex_history_unify_backup(app: AppHandle) -> Result<bool, CommandError> {
    let state = state(&app)?;
    blocking(move || Ok(codex_history_unify::has_backup(&state.root()))).await
}

#[tauri::command]
pub(crate) async fn restore_codex_history_from_backups(
    app: AppHandle,
) -> Result<RestoreOutcome, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let codex_root = codex_root(&state)?;
        let config = config_text(&state);
        codex_history_unify::restore_from_backups(state.root(), &codex_root, &config)
            .map_err(store_error)
    })
    .await
}
