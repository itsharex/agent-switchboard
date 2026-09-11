//! Active-save transaction for the current Codex-only provider contract.
use super::plan::{build_codex_plan, execute_projection, preview_projection};
use crate::commands::error::{require_write_confirmation, CommandError};
use crate::config_store::PendingProfileSave;
use asb_core::contracts::{
    CodexProviderDraft, CodexProviderFile, CodexProviderRecord, ProfileSaveKind,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};
use uuid::Uuid;

const PREPARATION_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_PREPARATIONS: usize = 32;

#[derive(Clone, Default)]
pub(crate) struct CodexProfileSavePreparations {
    inner: Arc<PreparationInner>,
}

#[derive(Default)]
struct PreparationInner {
    entries: Mutex<HashMap<String, PreparedCodexProfileSave>>,
    commit_lock: Mutex<()>,
}

pub(super) struct PreparedCodexProfileSave {
    pub(super) created_at: Instant,
    pub(super) profile_id: String,
    pub(super) expected_file_hash: String,
    pub(super) draft: CodexProviderDraft,
    pub(super) kind: ProfileSaveKind,
    pub(super) preview: Option<asb_switch::FilePreview>,
}

impl CodexProfileSavePreparations {
    pub(super) fn issue(&self, prepared: PreparedCodexProfileSave) -> Result<String, CommandError> {
        let mut entries = self.inner.entries.lock().map_err(|_| unavailable())?;
        let now = Instant::now();
        entries
            .retain(|_, entry| now.saturating_duration_since(entry.created_at) <= PREPARATION_TTL);
        if entries.len() >= MAX_PREPARATIONS {
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.created_at)
                .map(|(id, _)| id.clone())
            {
                entries.remove(&oldest);
            }
        }
        let id = Uuid::new_v4().to_string();
        entries.insert(id.clone(), prepared);
        Ok(id)
    }

    pub(super) fn take(&self, id: &str) -> Result<PreparedCodexProfileSave, CommandError> {
        let mut entries = self.inner.entries.lock().map_err(|_| unavailable())?;
        let prepared = entries.remove(id).ok_or_else(stale)?;
        if Instant::now().saturating_duration_since(prepared.created_at) > PREPARATION_TTL {
            return Err(stale());
        }
        Ok(prepared)
    }

    pub(super) fn lock_commit(&self) -> Result<MutexGuard<'_, ()>, CommandError> {
        self.inner.commit_lock.lock().map_err(|_| unavailable())
    }
}

pub(super) fn prepare_data(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    profile_id: &str,
    draft: &CodexProviderDraft,
    expected_file_hash: &str,
) -> Result<(ProfileSaveKind, Option<asb_switch::FilePreview>), CommandError> {
    let (_, candidate, kind) = classify(state, gateway, profile_id, draft, expected_file_hash)?;
    let preview = if kind == ProfileSaveKind::SaveAndApply {
        Some(preview_projection(
            state,
            &build_codex_plan(state, gateway, candidate)?,
        )?)
    } else {
        None
    };
    Ok((kind, preview))
}

pub(super) fn commit(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    prepared: &PreparedCodexProfileSave,
    confirm_write: bool,
) -> Result<CodexProviderRecord, CommandError> {
    let (stored, candidate, actual_kind) = classify(
        state,
        gateway,
        &prepared.profile_id,
        &prepared.draft,
        &prepared.expected_file_hash,
    )?;
    if actual_kind != prepared.kind {
        return Err(stale());
    }
    match actual_kind {
        ProfileSaveKind::NoChange => Ok(stored),
        ProfileSaveKind::SaveOnly => state
            .configuration()
            .update_codex_provider(
                &prepared.profile_id,
                prepared.draft.clone(),
                &stored.file_hash,
            )
            .map_err(save_failed),
        ProfileSaveKind::SaveAndApply => {
            require_write_confirmation(confirm_write, "保存并应用 Codex 供应商")?;
            let preview = prepared.preview.as_ref().ok_or_else(stale)?;
            apply(state, gateway, stored, candidate, &prepared.draft, preview)
        }
        ProfileSaveKind::Create => Err(CommandError::new(
            "codex-profile-save-invalid",
            "Codex 新建供应商不使用活跃保存事务",
        )),
    }
}

fn classify(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    profile_id: &str,
    draft: &CodexProviderDraft,
    expected_file_hash: &str,
) -> Result<(CodexProviderRecord, CodexProviderFile, ProfileSaveKind), CommandError> {
    let stored = state
        .configuration()
        .list_codex_providers()
        .map_err(|error| CommandError::new("codex-profile-not-found", error.to_string()))?
        .into_iter()
        .find(|record| record.profile.id == profile_id)
        .ok_or_else(|| CommandError::new("codex-profile-not-found", "Codex 供应商不存在"))?;
    if stored.file_hash != expected_file_hash {
        return Err(CommandError::new(
            "codex-profile-save-stale",
            "Codex 供应商文件已被外部修改，请重新读取后再保存",
        ));
    }
    let current = state
        .configuration()
        .find_codex_provider_file(profile_id)
        .map_err(|error| CommandError::new("codex-profile-not-found", error.to_string()))?;
    let candidate = draft
        .clone()
        .into_file(profile_id.to_string(), current.position);
    candidate
        .validate()
        .map_err(|error| CommandError::new("codex-profile-save-invalid", error))?;
    if candidate == current {
        return Ok((stored, candidate, ProfileSaveKind::NoChange));
    }
    // The catalog and protocol capabilities are not all represented in the
    // client TOML, but they do define the active gateway snapshot. Any change
    // to route facts or client-owned parameters must therefore re-apply an
    // active Codex route; local notes and usage metadata do not.
    let live_change =
        current.profile != candidate.profile || current.parameters != candidate.parameters;
    let active = live_change
        && crate::commands::config_status_report(state, gateway)?
            .into_iter()
            .find(|status| status.app == asb_core::AppKind::Codex)
            .and_then(|status| status.active_profile_id)
            .as_deref()
            == Some(profile_id);
    let kind = if active {
        ProfileSaveKind::SaveAndApply
    } else {
        ProfileSaveKind::SaveOnly
    };
    Ok((stored, candidate, kind))
}

fn apply(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    stored: CodexProviderRecord,
    candidate: CodexProviderFile,
    draft: &CodexProviderDraft,
    preview: &asb_switch::FilePreview,
) -> Result<CodexProviderRecord, CommandError> {
    let current = state
        .configuration()
        .find_codex_provider_file(&stored.profile.id)
        .map_err(|error| CommandError::new("codex-profile-save-failed", error.to_string()))?;
    let projection = build_codex_plan(state, gateway, candidate.clone())?;
    super::profile_rollback::save_codex(
        state,
        &current,
        &candidate,
        &preview.content_hash,
        &preview.rendered_hash,
    )
    .map_err(recovery_required)?;
    state
        .configuration()
        .begin_profile_save(&PendingProfileSave {
            profile_id: stored.profile.id.clone(),
            app: asb_core::AppKind::Codex,
            previous_file_hash: stored.file_hash.clone(),
        })
        .map_err(recovery_required)?;
    let saved = match state.configuration().update_codex_provider(
        &stored.profile.id,
        draft.clone(),
        &stored.file_hash,
    ) {
        Ok(saved) => saved,
        Err(error) => return clear_failed_save(state, error),
    };
    if let Err(error) = execute_projection(
        state,
        gateway,
        &projection,
        &preview.content_hash,
        &preview.rendered_hash,
    ) {
        if error.code == "config-recovery-required" {
            return Err(error);
        }
        return Err(rollback(
            state,
            gateway,
            &stored.profile.id,
            &saved.file_hash,
            error,
        ));
    }
    state
        .configuration()
        .clear_profile_save()
        .map_err(recovery_required)?;
    super::profile_rollback::clear(state).map_err(recovery_required)?;
    Ok(saved)
}

fn clear_failed_save(
    state: &crate::local_state::LocalState,
    update_error: String,
) -> Result<CodexProviderRecord, CommandError> {
    state
        .configuration()
        .clear_profile_save()
        .and_then(|_| super::profile_rollback::clear(state))
        .map_err(|clear_error| recovery_required(format!("{update_error}；{clear_error}")))?;
    Err(save_failed(update_error))
}

fn rollback(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    profile_id: &str,
    saved_hash: &str,
    original: CommandError,
) -> CommandError {
    match super::profile_rollback::restore(state, profile_id, saved_hash) {
        Ok(()) => match super::transaction::recover(state, gateway)
            .and_then(|_| state.configuration().clear_profile_save())
            .and_then(|_| super::profile_rollback::clear(state))
        {
            Ok(()) => original,
            Err(error) => recovery_required(format!(
                "{}；Codex 供应商档案已回滚，但{error}",
                original.message
            )),
        },
        Err(error) => recovery_required(format!(
            "{}；Codex 供应商未能回滚：{error}；已保留恢复记录以继续完成已确认的保存",
            original.message
        )),
    }
}

fn unavailable() -> CommandError {
    CommandError::new(
        "codex-profile-save-unavailable",
        "Codex 供应商保存准备状态不可用",
    )
}

fn stale() -> CommandError {
    CommandError::new(
        "codex-profile-save-stale",
        "Codex 保存预览已失效，请重新保存并查看最新差异",
    )
}

fn save_failed(message: String) -> CommandError {
    CommandError::new("codex-profile-save-failed", message)
}

fn recovery_required(message: impl Into<String>) -> CommandError {
    CommandError::new("codex-profile-save-recovery-required", message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::contracts::{
        CodexCapabilities, CodexCatalogEntry, CodexEndpoint, CodexModelRoute, CodexUpstream,
    };

    fn draft() -> CodexProviderDraft {
        CodexProviderDraft {
            name: "relay".to_string(),
            endpoint: CodexEndpoint("https://relay.example/v1".to_string()),
            api_key: "fixture-key".to_string(),
            upstream: CodexUpstream::Responses,
            request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
            default_model: "codex".to_string(),
            catalog: vec![CodexCatalogEntry {
                id: "codex".to_string(),
                context_window: 128_000,
                max_output_tokens: 16_384,
                function_tools: true,
                custom_tools: true,
                tool_search: true,
                reasoning: true,
                default_reasoning_level: asb_core::contracts::CodexReasoningLevel::High,
                supported_reasoning_levels: vec![
                    asb_core::contracts::CodexReasoningLevel::None,
                    asb_core::contracts::CodexReasoningLevel::High,
                ],
                images: true,
                compact: true,
            }],
            model_routes: vec![CodexModelRoute {
                client_model: "codex".to_string(),
                upstream_model: "vendor-codex".to_string(),
            }],
            capabilities: CodexCapabilities {
                responses: true,
                compact: true,
                models: true,
                chat_completions: true,
                alpha_search: false,
                image_generation: false,
                image_edit: false,
                function_tools: true,
                custom_tools: true,
                tool_search: true,
                reasoning: true,
                chat_reasoning: asb_core::contracts::CodexChatReasoning::Unsupported,
            },
            parameters: asb_core::ownership::default_provider_parameters(asb_core::AppKind::Codex),
            notes: None,
            website_url: None,
            usage_query: None,
        }
    }

    #[test]
    fn active_catalog_change_requires_a_projection_preview() {
        let _paths = crate::test_client_paths::redirect_client_paths();
        let directory = tempfile::tempdir().unwrap();
        let state = crate::local_state::LocalState::from_root(directory.path().join("state"));
        let gateway = crate::gateway::GatewayController::start(&state);
        let record = state
            .configuration()
            .create_codex_provider(draft())
            .unwrap();
        let target = state.target(asb_core::AppKind::Codex).unwrap();
        std::fs::write(&target, "model_provider = 'openai'\n").unwrap();
        std::fs::write(
            target.with_file_name("auth.json"),
            r#"{"auth_mode":"chatgpt","tokens":{"access_token":"at","refresh_token":"rt","id_token":"id"}}"#,
        )
        .unwrap();
        let projection = super::build_codex_plan(
            &state,
            &gateway,
            state
                .configuration()
                .find_codex_provider_file(&record.profile.id)
                .unwrap(),
        )
        .unwrap();
        let preview = super::preview_projection(&state, &projection).unwrap();
        super::execute_projection(
            &state,
            &gateway,
            &projection,
            &preview.content_hash,
            &preview.rendered_hash,
        )
        .unwrap();

        let mut changed = draft();
        changed.catalog[0].context_window = 256_000;
        let (kind, preview) = prepare_data(
            &state,
            &gateway,
            &record.profile.id,
            &changed,
            &record.file_hash,
        )
        .unwrap();
        assert_eq!(kind, ProfileSaveKind::SaveAndApply);
        assert!(preview.is_some());
        gateway.shutdown();
    }
}
