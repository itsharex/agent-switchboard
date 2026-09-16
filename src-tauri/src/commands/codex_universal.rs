//! Universal connection commands (P11): shared connections authored once and
//! fanned out into the specialized Codex profile store. Generation and sync
//! rewrite only universal-owned profile facts; local overrides stay put and
//! the two store contracts never merge.

use super::error::{blocking, operation_error, state, CommandError};
use crate::config_store::universal_providers;
use crate::local_state::LocalState;
use asb_core::contracts::{CodexUpstream, UniversalProvider, UniversalProviderInput};
use serde::Serialize;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexUniversalView {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) endpoint: String,
    pub(crate) upstream: CodexUpstream,
    pub(crate) has_api_key: bool,
    pub(crate) default_model: String,
    pub(crate) model_count: usize,
    pub(crate) generated_codex_profile_id: Option<String>,
    pub(crate) linked_profile_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexUniversalList {
    pub(crate) revision: String,
    pub(crate) providers: Vec<CodexUniversalView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexUniversalSyncOutcome {
    pub(crate) created: bool,
    pub(crate) unchanged: bool,
    pub(crate) warnings: Vec<String>,
    pub(crate) profile_id: String,
    pub(crate) list: CodexUniversalList,
}

fn store_error(error: String) -> CommandError {
    CommandError::new("codex-universal-store", error)
}

fn build_list(
    state: &LocalState,
    providers: Vec<UniversalProvider>,
    revision: String,
) -> CodexUniversalList {
    let linked_names: Vec<(String, String)> = state
        .configuration()
        .list_codex_providers()
        .map(|records| {
            records
                .into_iter()
                .map(|record| (record.profile.id, record.profile.name))
                .collect()
        })
        .unwrap_or_default();
    let providers = providers
        .into_iter()
        .map(|provider| {
            let linked_profile_name =
                provider
                    .generated_codex_profile_id
                    .as_deref()
                    .and_then(|id| {
                        linked_names
                            .iter()
                            .find(|(profile_id, _)| profile_id == id)
                            .map(|(_, name)| name.clone())
                    });
            CodexUniversalView {
                id: provider.id,
                linked_profile_name,
                name: provider.name,
                endpoint: provider.endpoint.0,
                upstream: provider.upstream,
                has_api_key: !provider.api_key.is_empty(),
                default_model: provider.default_model,
                model_count: provider.catalog.len(),
                generated_codex_profile_id: provider.generated_codex_profile_id,
            }
        })
        .collect();
    CodexUniversalList {
        revision,
        providers,
    }
}

/// Shared update/creation facts from the UI. `upstream` arrives as the
/// camelCase contract spelling; everything else is trimmed by the input
/// contract in core.
struct UniversalFacts {
    name: String,
    endpoint: String,
    upstream: CodexUpstream,
    api_key: String,
    models: Vec<String>,
}

fn facts(
    name: String,
    endpoint: String,
    upstream: CodexUpstream,
    api_key: String,
    models: Vec<String>,
) -> UniversalFacts {
    UniversalFacts {
        name,
        endpoint,
        upstream,
        api_key,
        models,
    }
}

#[tauri::command]
pub(crate) async fn list_codex_universal_providers(
    app: AppHandle,
) -> Result<CodexUniversalList, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let (providers, revision) = universal_providers::load(state.root()).map_err(store_error)?;
        Ok(build_list(&state, providers, revision))
    })
    .await
}

#[tauri::command]
pub(crate) async fn create_codex_universal_provider(
    app: AppHandle,
    name: String,
    endpoint: String,
    upstream: CodexUpstream,
    api_key: String,
    models: Vec<String>,
) -> Result<CodexUniversalList, CommandError> {
    let state = state(&app)?;
    let facts = facts(name, endpoint, upstream, api_key, models);
    blocking(move || {
        let (providers, revision) = universal_providers::load(state.root()).map_err(store_error)?;
        let input = UniversalProviderInput {
            name: facts.name,
            endpoint: facts.endpoint,
            upstream: facts.upstream,
            api_key: facts.api_key,
            authentication: None,
            models: facts.models,
        };
        let provider = input
            .into_provider(uuid::Uuid::new_v4().to_string())
            .map_err(store_error)?;
        let mut providers = providers;
        providers.push(provider);
        let revision = universal_providers::save(state.root(), providers.clone(), &revision)
            .map_err(store_error)?;
        Ok(build_list(&state, providers, revision))
    })
    .await
}

#[tauri::command]
pub(crate) async fn update_codex_universal_provider(
    app: AppHandle,
    id: String,
    expected_revision: String,
    name: String,
    endpoint: String,
    upstream: CodexUpstream,
    api_key: String,
    models: Vec<String>,
) -> Result<CodexUniversalList, CommandError> {
    let state = state(&app)?;
    let facts = facts(name, endpoint, upstream, api_key, models);
    blocking(move || {
        let (providers, _) = universal_providers::load(state.root()).map_err(store_error)?;
        let mut providers = providers;
        let existing = providers
            .iter_mut()
            .find(|provider| provider.id == id)
            .ok_or_else(|| store_error("通用连接不存在，请刷新后重试".into()))?;
        let link = existing.generated_codex_profile_id.clone();
        let input = UniversalProviderInput {
            name: facts.name,
            endpoint: facts.endpoint,
            upstream: facts.upstream,
            api_key: facts.api_key,
            authentication: existing.authentication,
            models: facts.models,
        };
        // Facts are replaced wholesale; the generated-profile link is store
        // state, not a user-editable fact.
        let updated = input.into_provider(id).map_err(store_error)?;
        *existing = UniversalProvider {
            generated_codex_profile_id: link,
            ..updated
        };
        let revision =
            universal_providers::save(state.root(), providers.clone(), &expected_revision)
                .map_err(store_error)?;
        Ok(build_list(&state, providers, revision))
    })
    .await
}

#[tauri::command]
pub(crate) async fn delete_codex_universal_provider(
    app: AppHandle,
    id: String,
    expected_revision: String,
) -> Result<CodexUniversalList, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let (providers, _) = universal_providers::load(state.root()).map_err(store_error)?;
        let mut providers = providers;
        let before = providers.len();
        providers.retain(|provider| provider.id != id);
        if providers.len() == before {
            return Err(store_error("通用连接不存在，请刷新后重试".into()));
        }
        let revision =
            universal_providers::save(state.root(), providers.clone(), &expected_revision)
                .map_err(store_error)?;
        Ok(build_list(&state, providers, revision))
    })
    .await
}

/// Generates the Codex projection when missing, otherwise syncs the
/// universal-owned facts into the linked profile. A no-op sync reports
/// `unchanged` without touching the profile file.
#[tauri::command]
pub(crate) async fn sync_codex_universal_profile(
    app: AppHandle,
    id: String,
    expected_revision: String,
) -> Result<CodexUniversalSyncOutcome, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let (providers, revision) = universal_providers::load(state.root()).map_err(store_error)?;
        if revision != expected_revision {
            return Err(store_error("通用连接已变化，请刷新后重试".into()));
        }
        let universal = providers
            .iter()
            .find(|provider| provider.id == id)
            .cloned()
            .ok_or_else(|| store_error("通用连接不存在，请刷新后重试".into()))?;
        universal.validate().map_err(store_error)?;
        let (created, unchanged, warnings, profile_id) = match &universal.generated_codex_profile_id
        {
            Some(profile_id) => {
                let (file, file_hash) = state
                    .configuration()
                    .find_codex_provider_with_revision(profile_id)
                    .map_err(|_| {
                        store_error("关联的 Codex 档案已被删除，请删除本连接后重新生成".into())
                    })?;
                let mut projected = file.clone();
                let warnings =
                    universal_providers::project_universal_into_codex(&universal, &mut projected);
                if projected == file {
                    (false, true, warnings, profile_id.clone())
                } else {
                    let record = state
                        .configuration()
                        .update_codex_provider_file(projected, &file_hash)
                        .map_err(|error| operation_error("codex-universal-sync", error))?;
                    (false, false, warnings, record.profile.id)
                }
            }
            None => {
                let record = state
                    .configuration()
                    .create_codex_provider(universal.to_codex_provider())
                    .map_err(|error| operation_error("codex-universal-sync", error))?;
                let profile_id = record.profile.id;
                let mut providers = providers;
                if let Some(linked) = providers
                    .iter_mut()
                    .find(|provider| provider.id == universal.id)
                {
                    linked.generated_codex_profile_id = Some(profile_id.clone());
                }
                universal_providers::save(state.root(), providers, &revision)
                    .map_err(store_error)?;
                (true, false, Vec::new(), profile_id)
            }
        };
        let (providers, revision) = universal_providers::load(state.root()).map_err(store_error)?;
        Ok(CodexUniversalSyncOutcome {
            created,
            unchanged,
            warnings,
            profile_id,
            list: build_list(&state, providers, revision),
        })
    })
    .await
}
