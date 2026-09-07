//! MCP connection checks: each check runs against the local capability
//! token surface and reports a redacted summary.

use asb_core::extensions::contracts::{ExtensionPayload, ExtensionTarget, McpCheckResult};
use serde::Serialize;
use tauri::AppHandle;

use super::support::*;
use crate::commands::error::{state, CommandError};
use crate::extensions::checks::{self};
use crate::extensions::secrets::SecretBackend;
use crate::extensions::secrets::SystemSecrets;

// --------------------------------------------------------------- connection checks

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckStartedDto {
    check_id: String,
}

#[tauri::command]
pub async fn check_mcp_connection(
    app: AppHandle,
    definition_id: String,
    target: ExtensionTarget,
    confirm: bool,
) -> Result<CheckStartedDto, CommandError> {
    if !confirm {
        return Err(CommandError::new(
            "check-not-confirmed",
            "连接检测会运行服务命令或访问端点；需要明确确认",
        ));
    }
    let state = state(&app)?;
    let store = extension_store(&state);
    let definition = store
        .get_definition(&definition_id)
        .map_err(store_error)?
        .ok_or_else(|| CommandError::new("extension-not-found", "扩展不存在或已被删除"))?;
    let ExtensionPayload::Mcp(mcp) = definition.payload else {
        return Err(CommandError::new(
            "extension-invalid",
            "连接检测只适用于 MCP 服务",
        ));
    };
    let diagnostics = checks::static_diagnostics(&mcp);
    let _ = diagnostics;
    let (check_id, flag) = check_registry().start();
    let check_id_out = check_id.clone();
    let thread_check_id = check_id.clone();
    let definition_revision = definition.revision;
    let store_for_check = store.clone();
    std::thread::spawn(move || {
        let result = checks::run_check_for(
            &definition_id,
            definition_revision,
            &target,
            &thread_check_id,
            check_registry(),
            &mcp,
            &|reference| SystemSecrets.get(reference).ok(),
        );
        let completed = store_for_check
            .save_check(&result)
            .map(|()| result)
            .map_err(store_error);
        check_results()
            .lock()
            .expect("checks")
            .insert(check_id, completed);
    });
    let _ = flag;
    Ok(CheckStartedDto {
        check_id: check_id_out,
    })
}

#[tauri::command]
pub async fn get_mcp_check(check_id: String) -> Result<Option<McpCheckResult>, CommandError> {
    match check_results().lock().expect("checks").remove(&check_id) {
        Some(Ok(result)) => Ok(Some(result)),
        Some(Err(error)) => Err(error),
        None => Ok(None),
    }
}

#[tauri::command]
pub async fn cancel_mcp_check(check_id: String) -> Result<bool, CommandError> {
    Ok(check_registry().cancel(&check_id))
}
