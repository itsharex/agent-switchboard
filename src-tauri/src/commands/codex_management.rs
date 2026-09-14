//! Codex-specific provider actions; no Claude profile conversion or live-file writes.
use super::error::{blocking, observe, operation_error, state, CommandError};
use super::ConfigWriteGate;
use asb_core::codex_presets::{CodexPresetPreparation, CodexPresetSummary};
use asb_core::contracts::CodexProviderRecord;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub(crate) async fn list_codex_presets() -> Result<Vec<CodexPresetSummary>, CommandError> {
    asb_core::codex_presets::list()
        .map_err(|error| CommandError::new("codex-presets-invalid", error))
}

#[tauri::command]
pub(crate) async fn prepare_codex_preset(
    preset_id: String,
    api_key: String,
) -> Result<CodexPresetPreparation, CommandError> {
    asb_core::codex_presets::prepare(&preset_id, &api_key)
        .map_err(|error| CommandError::new("codex-preset-invalid", error))
}

#[tauri::command]
pub(crate) async fn search_codex_profiles(
    app: AppHandle,
    query: String,
) -> Result<Vec<CodexProviderRecord>, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        state
            .configuration()
            .search_codex_providers(&query)
            .map_err(|error| operation_error("codex-profile-search-failed", error))
    })
    .await
}

#[tauri::command]
pub(crate) async fn duplicate_codex_profile(
    app: AppHandle,
    profile_id: String,
    expected_file_hash: String,
    name: Option<String>,
) -> Result<CodexProviderRecord, CommandError> {
    let refresh = app.clone();
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    let result = observe(
        crate::runtime_log::RuntimeLogAction::ProfileCreated,
        async move {
            let state = state(&app)?;
            blocking(move || {
                let _guard = gate
                    .lock()
                    .map_err(|error| CommandError::new("codex-profile-copy-gate", error))?;
                state
                    .configuration()
                    .duplicate_codex_provider(&profile_id, &expected_file_hash, name)
                    .map_err(|error| operation_error("codex-profile-copy-failed", error))
            })
            .await
        },
    )
    .await;
    if result.is_ok() {
        crate::tray::refresh(&refresh);
    }
    result
}

/// A native Responses profile may be active without an in-memory gateway route.
/// The generated catalog reference still identifies its owning provider.
pub(crate) fn ensure_deletable(
    state: &crate::local_state::LocalState,
    id: &str,
) -> Result<(), CommandError> {
    let (policy, _) = crate::gateway::codex::policy::load(state.root())
        .map_err(|error| CommandError::new("codex-policy-invalid", error))?;
    if policy.provider_ids.iter().any(|entry| entry == id) {
        return Err(CommandError::new(
            "codex-provider-in-failover-queue",
            "请先从 Codex 故障转移队列移除该供应商，再删除档案",
        ));
    }
    let target = state
        .target(asb_core::AppKind::Codex)
        .map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let text = match std::fs::read_to_string(target) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => {
            return Err(CommandError::new(
                "codex-profile-delete-check-failed",
                "无法读取 Codex 配置以确认档案未被引用",
            ))
        }
    };
    let document = text.parse::<toml_edit::DocumentMut>().map_err(|_| {
        CommandError::new(
            "codex-profile-delete-check-failed",
            "Codex 配置格式无效，无法确认档案是否仍被引用",
        )
    })?;
    if document
        .get("model_catalog_json")
        .and_then(toml_edit::Item::as_str)
        .is_some_and(|pointer| references_profile(pointer, id))
    {
        return Err(CommandError::new(
            "codex-profile-active",
            "该 Codex 供应商仍被当前客户端配置引用；请先切换后再删除",
        ));
    }
    Ok(())
}
fn references_profile(pointer: &str, id: &str) -> bool {
    std::path::Path::new(pointer)
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| {
            name.starts_with(&format!("agent-switchboard-codex-{id}-")) && name.ends_with(".json")
        })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_active_profile_reference_is_independent_of_gateway_or_revision() {
        let id = "00000000-0000-0000-0000-000000000001";
        assert!(references_profile(
            &format!("agent-switchboard-codex-{id}-oldrevision.json"),
            id
        ));
        assert!(references_profile(
            &format!("/isolated/catalogs/agent-switchboard-codex-{id}-newrevision.json"),
            id
        ));
        assert!(!references_profile(
            "agent-switchboard-codex-other-revision.json",
            id
        ));
        assert!(!references_profile("my-own-model-catalog.json", id));
    }
}
