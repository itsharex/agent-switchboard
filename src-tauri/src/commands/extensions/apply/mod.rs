//! Applying a confirmed plan and replaying history: the executor runs
//! journaled steps under target locks, the library commits inside the
//! journal boundary, and restore inverts a finished operation's exact
//! completed steps.

use asb_core::extensions::contracts::{
    ExtensionBinding, ExtensionOperationRecord, OperationSnapshot, PlanOperation,
    EXTENSIONS_SCHEMA_VERSION,
};
use serde::Serialize;
use tauri::AppHandle;

use super::support::*;
use crate::commands::error::{blocking, require_write_confirmation, state, CommandError};
use crate::extensions::secrets::SecretBackend;
use crate::extensions::secrets::SystemSecrets;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutcomeDto {
    /// The finished record on success or on a rolled-back failure.
    record: Option<ExtensionOperationRecord>,
    /// Set when the batch was rejected outright (stale plan or lock); no
    /// client file changed.
    rejected: Option<String>,
    rolled_back: bool,
}

#[tauri::command]
pub async fn apply_extension_plan(
    app: AppHandle,
    plan_id: String,
    confirm_write: bool,
) -> Result<ApplyOutcomeDto, CommandError> {
    require_write_confirmation(confirm_write, "应用扩展变更")?;
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let pending = pending_plans()
            .lock()
            .expect("plans")
            .remove(&plan_id)
            .ok_or_else(|| CommandError::new("plan-expired", "计划不存在或已应用；请重新预览"))?;

        // Re-verify the plan's library preconditions before anything runs:
        // the generation must be unchanged since preparation, and every
        // referenced credential must still resolve to the previewed value.
        // (The validity window is enforced by the executor under lock.)
        if store.manifest().map_err(store_error)?.generation
            != pending.plan.preconditions.generation
        {
            return Ok(ApplyOutcomeDto {
                record: None,
                rejected: Some("扩展库已发生变化；请重新预览后应用".to_string()),
                rolled_back: false,
            });
        }
        for (reference, expected_digest) in &pending.plan.preconditions.secret_digests {
            match SystemSecrets.get(reference) {
                Ok(value) => {
                    let digest = {
                        use sha2::Digest;
                        sha2::Sha256::digest(value.as_bytes())
                            .iter()
                            .map(|byte| format!("{byte:02x}"))
                            .collect::<String>()
                    };
                    if &digest != expected_digest {
                        return Ok(ApplyOutcomeDto {
                            record: None,
                            rejected: Some(format!("凭据 {reference} 已变化；请重新预览后应用")),
                            rolled_back: false,
                        });
                    }
                }
                Err(_) => {
                    return Ok(ApplyOutcomeDto {
                        record: None,
                        rejected: Some(format!("凭据引用 {reference} 无法解析；请重新预览")),
                        rolled_back: false,
                    });
                }
            }
        }

        // The journal carries the prepared library change (without the
        // record, which only exists once the executor finishes): a crash
        // between library writes and the commit marker restores the
        // pre-state from this payload.
        let journal_commit = crate::extensions::store::LibraryCommit {
            history_operation_id: pending.commit.history_operation_id.clone(),
            snapshot: None,
            binding_upserts: pending.commit.binding_upserts.clone(),
            baseline_files: pending.commit.baseline_files.clone(),
            baseline_deletes: pending.commit.baseline_deletes.clone(),
            binding_deletes: pending.commit.binding_deletes.clone(),
            pre_bindings: pending.commit.pre_bindings.clone(),
            pre_baselines: pending.commit.pre_baselines.clone(),
        };
        let journal_commit_json = serde_json::to_value(&journal_commit)
            .map_err(|error| CommandError::new("extension-store", error.to_string()))?;
        let request = asb_switch::ExtensionApplyRequest {
            operation_id: &pending.operation_id,
            plan: &pending.plan,
            journal_dir: &pending.journal_dir,
            library_commit: Some(journal_commit_json),
        };
        let pre_bindings = pending.commit.pre_bindings.clone();
        let pre_baselines = pending.commit.pre_baselines.clone();
        let outcome = asb_switch::apply_extension_plan(
            &asb_switch::FsIo,
            &request,
            |record: &ExtensionOperationRecord,
             completed_steps: &[asb_core::extensions::contracts::AppliedExtensionStep]| {
                // One locked library batch: bindings, baselines, deletions,
                // the operation snapshot, and the generation bump commit or
                // fail together.
                let mut applied = journal_commit.clone();
                for binding in &mut applied.binding_upserts {
                    if let Some(revision) = pending
                        .plan
                        .operations
                        .iter()
                        .find(|operation| operation.definition_id == binding.resource_id)
                        .map(|operation| operation.definition_revision)
                    {
                        binding.last_applied_revision = Some(revision);
                    }
                    binding.updated_at = now();
                }
                let mut post_bindings: Vec<ExtensionBinding> = applied
                    .binding_upserts
                    .iter()
                    .filter(|binding| !applied.binding_deletes.contains(&binding.id))
                    .cloned()
                    .collect();
                // Baseline-only updates keep the binding itself unchanged,
                // but the operation snapshot must still describe its
                // complete post-state for a later historical restore.
                for binding in &pre_bindings {
                    if !applied.binding_deletes.contains(&binding.id)
                        && !post_bindings.iter().any(|post| post.id == binding.id)
                    {
                        post_bindings.push(binding.clone());
                    }
                }
                let post_baselines = applied
                    .baseline_files
                    .iter()
                    .filter(|(binding_id, _)| !applied.binding_deletes.contains(binding_id))
                    .cloned()
                    .collect();
                applied.snapshot = Some(OperationSnapshot {
                    schema_version: EXTENSIONS_SCHEMA_VERSION,
                    record: record.clone(),
                    completed_steps: completed_steps.to_vec(),
                    pre_bindings: pre_bindings.clone(),
                    pre_baselines: pre_baselines.clone(),
                    post_bindings,
                    post_baselines,
                });
                let is_restore = pending
                    .plan
                    .operations
                    .iter()
                    .any(|operation| operation.operation == PlanOperation::Restore);
                let commit_result = if is_restore {
                    store.commit_restore_if_current(
                        &applied,
                        pending.plan.preconditions.generation,
                    )
                } else {
                    let expected_revisions: Vec<(String, u64)> = pending
                        .plan
                        .operations
                        .iter()
                        .map(|operation| {
                            (operation.definition_id.clone(), operation.definition_revision)
                        })
                        .collect();
                    store.commit_batch_if_current(
                        &applied,
                        pending.plan.preconditions.generation,
                        &expected_revisions,
                    )
                };
                commit_result.map_err(|error| match error {
                        crate::extensions::store::ExtensionStoreError::RecoveryRequired(
                            message,
                        ) => asb_switch::LibraryCommitError::recovery_required(message),
                        other => asb_switch::LibraryCommitError::failed(other.to_string()),
                    })
            },
        );
        match &outcome {
            Ok(record) => Ok(ApplyOutcomeDto {
                record: Some(record.clone()),
                rejected: None,
                rolled_back: false,
            }),
            Err(asb_switch::ExtensionError::PlanStale { message })
            | Err(asb_switch::ExtensionError::Blocked { message })
            | Err(asb_switch::ExtensionError::Preparation { message }) => Ok(ApplyOutcomeDto {
                record: None,
                rejected: Some(message.clone()),
                rolled_back: false,
            }),
            Err(asb_switch::ExtensionError::Failed { record, .. }) => {
                // The client files rolled back; persist the failure record
                // without any library change.
                store
                    .append_snapshot(&OperationSnapshot {
                        schema_version: EXTENSIONS_SCHEMA_VERSION,
                        record: record.clone(),
                        completed_steps: Vec::new(),
                        pre_bindings: pending.commit.pre_bindings.clone(),
                        pre_baselines: pending.commit.pre_baselines.clone(),
                        post_bindings: pending.commit.pre_bindings.clone(),
                        post_baselines: pending.commit.pre_baselines.clone(),
                    })
                    .map_err(store_error)?;
                Ok(ApplyOutcomeDto {
                    record: Some(record.clone()),
                    rejected: None,
                    rolled_back: true,
                })
            }
            Err(asb_switch::ExtensionError::RecoveryRequired { message }) => {
                Err(CommandError::new("recovery-required", message.clone()))
            }
        }
    })
    .await
}

mod restore;
pub use restore::{
    __cmd__prepare_extension_restore, __tauri_command_name_prepare_extension_restore,
    prepare_extension_restore,
};

#[tauri::command]
pub async fn get_extension_operation(
    app: AppHandle,
    operation_id: String,
) -> Result<Option<ExtensionOperationRecord>, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let snapshot = store.get_snapshot(&operation_id).map_err(store_error)?;
        Ok(snapshot.map(|snapshot| snapshot.record))
    })
    .await
}
