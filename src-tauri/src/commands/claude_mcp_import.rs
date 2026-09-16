//! the source application MCP 服务导入命令（Claude 专属入口）：只读扫描 + 确认后写入扩展库。

use super::error::{blocking, require_write_confirmation, state, CommandError};
use crate::claude_mcp_source::{self, ClaudeMcpImportResult, ClaudeMcpSource};
use crate::extensions::store::ExtensionStore;
use tauri::AppHandle;

fn failure(message: impl Into<String>) -> CommandError {
    CommandError::new("claude-mcp-import-failed", message)
}

#[tauri::command]
pub(crate) async fn scan_claude_mcp_source(
    app: AppHandle,
    source_path: String,
) -> Result<ClaudeMcpSource, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        let library = ExtensionStore::from_state(&local)
            .list_definitions()
            .map_err(|error| failure(error.to_string()))?;
        claude_mcp_source::scan(std::path::Path::new(&source_path), &library).map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_claude_mcp_source(
    app: AppHandle,
    source_path: String,
    source_ids: Vec<String>,
    source_revision: String,
    confirm_write: bool,
) -> Result<ClaudeMcpImportResult, CommandError> {
    require_write_confirmation(confirm_write, "导入 MCP 服务到扩展库")?;
    let local = state(&app)?;
    blocking(move || {
        let store = ExtensionStore::from_state(&local);
        let library = store
            .list_definitions()
            .map_err(|error| failure(error.to_string()))?;
        let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let (definitions, result) = claude_mcp_source::plan_import(
            std::path::Path::new(&source_path),
            &source_ids,
            &source_revision,
            &library,
            &now,
            || format!("ext-{}", uuid::Uuid::new_v4().simple()),
        )
        .map_err(failure)?;
        for definition in &definitions {
            store
                .create_definition(definition)
                .map_err(|error| failure(error.to_string()))?;
        }
        Ok(result)
    })
    .await
}
