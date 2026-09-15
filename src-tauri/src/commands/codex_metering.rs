//! Codex accounting commands never read Claude records or write native client configuration.
use super::error::{blocking, require_write_confirmation, state, CommandError};
use crate::codex_metering::{
    self, CodexLedgerFilter, CodexLedgerPage, CodexLedgerSummary, CodexMeteringSettings,
    CodexMeteringSnapshot, CodexRequestLedger, CodexSessionSyncReport,
};
use serde::Serialize;
use tauri::{AppHandle, Manager};
fn failure(error: String) -> CommandError {
    CommandError::new("codex-metering-unavailable", error)
}

#[tauri::command]
pub(crate) async fn get_codex_metering(
    app: AppHandle,
) -> Result<CodexMeteringSnapshot, CommandError> {
    let local = state(&app)?;
    blocking(move || codex_metering::read_settings(local.root()).map_err(failure)).await
}
#[tauri::command]
pub(crate) async fn set_codex_metering(
    app: AppHandle,
    settings: CodexMeteringSettings,
    expected_revision: String,
    confirm_write: bool,
) -> Result<CodexMeteringSnapshot, CommandError> {
    require_write_confirmation(confirm_write, "保存 Codex 本地价格与限额")?;
    let local = state(&app)?;
    let gate = app.state::<super::ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(failure)?;
        let providers = local
            .configuration()
            .list_codex_providers()
            .map_err(super::error::store_error)?;
        for id in settings.providers.keys() {
            if !providers.iter().any(|provider| &provider.profile.id == id) {
                return Err(failure(
                    "Codex 计量规则只能引用当前 Codex 第三方供应商".into(),
                ));
            }
        }
        codex_metering::save_settings(local.root(), settings, &expected_revision).map_err(failure)
    })
    .await
}
#[tauri::command]
pub(crate) async fn get_codex_request_ledger(
    app: AppHandle,
    filter: Option<CodexLedgerFilter>,
    offset: u32,
    limit: u32,
) -> Result<CodexLedgerPage, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        CodexRequestLedger::new(local.root())
            .page(&filter.unwrap_or_default(), offset, limit)
            .map_err(failure)
    })
    .await
}
#[tauri::command]
pub(crate) async fn get_codex_request_summary(
    app: AppHandle,
    filter: Option<CodexLedgerFilter>,
) -> Result<CodexLedgerSummary, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        // Session usage syncs incrementally ahead of every summary so the
        // totals stay current without a separate refresh flow; per-file
        // problems are reported by the dedicated sync command.
        let report = codex_metering::sync_codex_session_usage(local.root());
        for error in &report.errors {
            log::warn!("Codex 会话用量同步：{error}");
        }
        CodexRequestLedger::new(local.root())
            .summary(&filter.unwrap_or_default())
            .map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn sync_codex_session_usage(
    app: AppHandle,
) -> Result<CodexSessionSyncReport, CommandError> {
    let local = state(&app)?;
    blocking(move || Ok(codex_metering::sync_codex_session_usage(local.root()))).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexSessionRebuildOutcome {
    pub backup_path: Option<std::path::PathBuf>,
    pub report: CodexSessionSyncReport,
}

#[tauri::command]
pub(crate) async fn rebuild_codex_session_usage(
    app: AppHandle,
    confirm_write: bool,
) -> Result<CodexSessionRebuildOutcome, CommandError> {
    require_write_confirmation(confirm_write, "重建 Codex 会话用量账本")?;
    let local = state(&app)?;
    blocking(move || {
        codex_metering::rebuild_codex_session_usage(local.root())
            .map(|(backup_path, report)| CodexSessionRebuildOutcome {
                backup_path,
                report,
            })
            .map_err(failure)
    })
    .await
}
#[tauri::command]
pub(crate) async fn reprice_codex_requests(
    app: AppHandle,
    filter: Option<CodexLedgerFilter>,
    expected_revision: String,
    confirm_write: bool,
) -> Result<u64, CommandError> {
    require_write_confirmation(confirm_write, "回填 Codex 历史请求费用")?;
    let local = state(&app)?;
    let gate = app.state::<super::ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(failure)?;
        let snapshot = codex_metering::read_settings(local.root()).map_err(failure)?;
        if snapshot.revision != expected_revision {
            return Err(failure("Codex 价格已改变，请重新确认后回填".into()));
        }
        CodexRequestLedger::new(local.root())
            .reprice(&filter.unwrap_or_default(), &snapshot.settings.prices)
            .map_err(failure)
    })
    .await
}
