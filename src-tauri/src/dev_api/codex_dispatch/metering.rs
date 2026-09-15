use super::super::http::{argument, as_json, InvokeRequest};
use crate::commands::{codex_metering, error::CommandError};
use serde_json::Value;
use tauri::AppHandle;
pub(super) async fn dispatch(
    app: &AppHandle,
    request: &InvokeRequest,
) -> Result<Option<Value>, CommandError> {
    let result = match request.command.as_str() {
        "get_codex_metering" => as_json(codex_metering::get_codex_metering(app.clone()).await),
        "set_codex_metering" => as_json(
            codex_metering::set_codex_metering(
                app.clone(),
                argument(&request.args, "settings")?,
                argument(&request.args, "expectedRevision")?,
                argument(&request.args, "confirmWrite")?,
            )
            .await,
        ),
        "get_codex_request_ledger" => as_json(
            codex_metering::get_codex_request_ledger(
                app.clone(),
                argument(&request.args, "filter")?,
                argument(&request.args, "offset")?,
                argument(&request.args, "limit")?,
            )
            .await,
        ),
        "get_codex_request_summary" => as_json(
            codex_metering::get_codex_request_summary(
                app.clone(),
                argument(&request.args, "filter")?,
            )
            .await,
        ),
        "reprice_codex_requests" => as_json(
            codex_metering::reprice_codex_requests(
                app.clone(),
                argument(&request.args, "filter")?,
                argument(&request.args, "expectedRevision")?,
                argument(&request.args, "confirmWrite")?,
            )
            .await,
        ),
        "sync_codex_session_usage" => {
            as_json(codex_metering::sync_codex_session_usage(app.clone()).await)
        }
        "rebuild_codex_session_usage" => as_json(
            codex_metering::rebuild_codex_session_usage(
                app.clone(),
                argument(&request.args, "confirmWrite")?,
            )
            .await,
        ),
        _ => return Ok(None),
    };
    result.map(Some)
}
