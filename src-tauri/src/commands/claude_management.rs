//! Claude provider operations own Claude drafts only; native Codex contracts never enter here.

use super::{
    error::{blocking, require_write_confirmation, state, CommandError},
    ConfigWriteGate,
};
use asb_core::{
    claude_presets::{self, ClaudePresetPreparation, ClaudePresetSummary},
    AppKind, ProviderDraft, ProviderRecord,
};
use std::collections::BTreeMap;
use tauri::{AppHandle, Manager};

fn failure(message: impl Into<String>) -> CommandError {
    CommandError::new("claude-provider-operation-failed", message)
}

/// The shared-snippet scan outcome together with the current client-settings
/// revision the confirmation must echo back.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeSnippetScanView {
    #[serde(flatten)]
    pub source: crate::claude_snippets::ClaudeSnippetSource,
    pub settings_revision: String,
}

#[tauri::command]
pub(crate) async fn scan_claude_snippet_source(
    app: AppHandle,
    source_path: String,
) -> Result<ClaudeSnippetScanView, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        let source =
            crate::claude_snippets::scan(std::path::Path::new(&source_path)).map_err(failure)?;
        let settings_revision = local
            .configuration()
            .get_client_settings(AppKind::Claude)
            .map(|snapshot| snapshot.settings_hash)
            .map_err(|error| failure(error.to_string()))?;
        Ok(ClaudeSnippetScanView {
            source,
            settings_revision,
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn import_claude_snippet_source(
    app: AppHandle,
    source_path: String,
    source_revision: String,
    expected_settings_hash: String,
    confirm_write: bool,
) -> Result<crate::claude_snippets::ClaudeSnippetImport, CommandError> {
    require_write_confirmation(confirm_write, "导入 Claude 通用配置片段")?;
    let local = state(&app)?;
    blocking(move || {
        super::switching::ensure_profile_save_recovered(&app)?;
        crate::claude_snippets::import(
            &local.configuration(),
            std::path::Path::new(&source_path),
            &source_revision,
            &expected_settings_hash,
        )
        .map_err(failure)
    })
    .await
}

#[tauri::command]
pub(crate) fn list_claude_presets() -> Result<Vec<ClaudePresetSummary>, CommandError> {
    claude_presets::list().map_err(failure)
}

#[tauri::command]
pub(crate) fn prepare_claude_preset(
    preset_id: String,
    api_key: String,
    variables: BTreeMap<String, String>,
    account_id: Option<String>,
) -> Result<ClaudePresetPreparation, CommandError> {
    claude_presets::prepare(&preset_id, &api_key, &variables, account_id.as_deref())
        .map_err(failure)
}

#[tauri::command]
pub(crate) async fn duplicate_claude_profile(
    app: AppHandle,
    profile_id: String,
    name: String,
    expected_file_hash: String,
    confirm_write: bool,
) -> Result<ProviderRecord, CommandError> {
    require_write_confirmation(confirm_write, "复制 Claude 供应商档案")?;
    let local = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(failure)?;
        duplicate(
            &local.configuration(),
            &profile_id,
            &name,
            &expected_file_hash,
        )
        .map_err(failure)
    })
    .await
}

fn duplicate(
    store: &crate::config_store::ConfigStore,
    id: &str,
    name: &str,
    hash: &str,
) -> Result<ProviderRecord, String> {
    let record = store
        .find_provider_record(id)
        .map_err(|error| error.to_string())?;
    if record.profile.app != AppKind::Claude {
        return Err("此操作只复制 Claude 档案".into());
    }
    if record.file_hash != hash {
        return Err("Claude 供应商已改变，请重新读取后再复制".into());
    }
    if record.profile.route_mode == asb_core::RouteMode::Official {
        return Err("Claude 官方登录为单一客户端入口，不能复制为第二个官方档案".into());
    }
    let mut value = serde_json::to_value(&record.profile).map_err(|_| "Claude 档案无法编码")?;
    let object = value.as_object_mut().ok_or("Claude 档案无效")?;
    object.remove("id");
    object.insert("name".into(), serde_json::json!(name));
    let draft: ProviderDraft = serde_json::from_value(value).map_err(|_| "Claude 档案无法复制")?;
    store
        .create_provider(draft)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) async fn search_claude_profiles(
    app: AppHandle,
    query: String,
) -> Result<Vec<ProviderRecord>, CommandError> {
    let local = state(&app)?;
    blocking(move || {
        let query = query.trim().to_lowercase();
        Ok(local
            .configuration()
            .list_providers()
            .map_err(|error| failure(error.to_string()))?
            .into_iter()
            .filter(|record| record.profile.app == AppKind::Claude)
            .filter(|record| {
                [
                    &record.profile.name,
                    record.profile.base_url.as_deref().unwrap_or(""),
                    record.profile.model.as_deref().unwrap_or(""),
                    record.profile.notes.as_deref().unwrap_or(""),
                ]
                .iter()
                .any(|value| value.to_lowercase().contains(&query))
            })
            .collect())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_preserves_configuration_without_sharing_identity_or_reusing_a_stale_revision() {
        let dir = tempfile::tempdir().unwrap();
        let store = crate::config_store::ConfigStore::new(dir.path().into());
        let prepared = claude_presets::prepare(
            "claude-preset-89",
            "fake-duplicate-key",
            &BTreeMap::new(),
            None,
        )
        .unwrap();
        let original = store.create_provider(prepared.draft).unwrap();
        let copy = duplicate(
            &store,
            &original.profile.id,
            "本地副本",
            &original.file_hash,
        )
        .unwrap();
        assert_ne!(copy.profile.id, original.profile.id);
        assert_eq!(copy.profile.connection, original.profile.connection);
        assert_eq!(copy.profile.api_key, original.profile.api_key);
        assert!(duplicate(&store, &original.profile.id, "旧副本", "stale").is_err());
        assert_eq!(
            store
                .find_provider_record(&original.profile.id)
                .unwrap()
                .file_hash,
            original.file_hash
        );
    }
}
