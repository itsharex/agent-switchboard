//! Claude price editing stays independent from client settings and Codex usage.

use super::error::{blocking, require_write_confirmation, state, CommandError};
use crate::gateway::claude_pricing::{read_snapshot, ClaudePriceBook, ClaudePriceBookSnapshot};
use tauri::Manager;
fn failure(error: String) -> CommandError {
    CommandError::new("claude-pricing-unavailable", error)
}

#[tauri::command]
pub(crate) async fn get_claude_price_book(
    app: tauri::AppHandle,
) -> Result<ClaudePriceBookSnapshot, CommandError> {
    let local = state(&app)?;
    blocking(move || read_snapshot(local.root()).map_err(failure)).await
}

#[tauri::command]
pub(crate) async fn set_claude_price_book(
    app: tauri::AppHandle,
    book: ClaudePriceBook,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<ClaudePriceBookSnapshot, CommandError> {
    require_write_confirmation(confirm_write, "修改 Claude 本地参考价格")?;
    let local = state(&app)?;
    let gate = app.state::<super::ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(failure)?;
        let current = read_snapshot(local.root()).map_err(failure)?;
        if current.file_hash != expected_file_hash {
            return Err(failure("Claude 价格表已改变，请重新读取".into()));
        }
        book.save(local.root()).map_err(failure)?;
        read_snapshot(local.root()).map_err(failure)
    })
    .await
}
