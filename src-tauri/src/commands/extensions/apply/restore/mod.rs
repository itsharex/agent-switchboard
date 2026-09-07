//! Historical restore: replaying a finished operation's completed steps
//! backwards, verified against the current client and library state.

use std::collections::BTreeMap;

use crate::extensions::store::LibraryCommit;
use asb_core::extensions::contracts::{ExtensionBinding, PlanOperation, EXTENSIONS_SCHEMA_VERSION};
use asb_core::extensions::plan::{redact_plan, ExtensionPlan, ExtensionPlanView, PlannedTarget};
use tauri::AppHandle;

use super::restore::invert::{
    append_restore_targets, current_baseline_files, ensure_restore_is_current,
};
use crate::commands::error::{blocking, state, CommandError};
use crate::commands::extensions::support::*;

#[tauri::command]
pub async fn prepare_extension_restore(
    app: AppHandle,
    operation_id: String,
) -> Result<ExtensionPlanView, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let snapshot = store
            .get_snapshot(&operation_id)
            .map_err(store_error)?
            .ok_or_else(|| CommandError::new("extension-not-found", "操作记录不存在"))?;
        let record = &snapshot.record;
        if snapshot.schema_version != EXTENSIONS_SCHEMA_VERSION
            || record.schema_version != EXTENSIONS_SCHEMA_VERSION
        {
            return Err(CommandError::new(
                "extension-unsupported-history",
                "该操作记录不是当前扩展格式，不能恢复",
            ));
        }
        if record.rollback.is_some()
            || record.resources.iter().any(|resource| {
                resource.targets.iter().any(|target| {
                    !matches!(
                        target.outcome,
                        asb_core::extensions::contracts::TargetOutcome::Applied
                    )
                })
            })
        {
            return Err(CommandError::new(
                "extension-not-restorable",
                "该操作没有完整应用，不能再次恢复",
            ));
        }
        ensure_restore_is_current(&store, &snapshot)?;

        let mut plan = ExtensionPlan {
            plan_id: String::new(),
            created_at: String::new(),
            // A restore reverses every resource the original batch touched;
            // the binding-level stubs carry per-resource identity below.
            operations: record
                .resources
                .iter()
                .map(|resource| asb_core::extensions::plan::PlannedOperation {
                    definition_id: resource.definition_id.clone(),
                    definition_revision: resource.definition_revision,
                    operation: PlanOperation::Restore,
                    targets: Vec::new(),
                })
                .collect(),
            preconditions: asb_core::extensions::plan::PlanPreconditions {
                expires_at: String::new(),
                generation: 0,
                secret_digests: BTreeMap::new(),
            },
        };
        let restore_operation_id = new_id("op");
        let backup_root = store.backup_root(&restore_operation_id);
        let journal_dir = store.transaction_dir(&restore_operation_id);

        // Library restore: back to the snapshot's pre-state. Bindings this
        // operation created are removed; pre-bindings and pre-baselines are
        // written back verbatim.
        let mut upserts: Vec<ExtensionBinding> = Vec::new();
        let mut deletes: Vec<String> = Vec::new();
        for post in &snapshot.post_bindings {
            match snapshot.pre_bindings.iter().find(|pre| pre.id == post.id) {
                Some(pre) => upserts.push(pre.clone()),
                None => deletes.push(post.id.clone()),
            }
        }
        // Bindings removed by the operation come back.
        for pre in &snapshot.pre_bindings {
            if !snapshot.post_bindings.iter().any(|post| post.id == pre.id) {
                upserts.push(pre.clone());
            }
        }

        append_restore_targets(&mut plan, &snapshot, record, &backup_root)?;
        if plan.planned_targets().next().is_none() {
            if let Some(binding) = snapshot
                .pre_bindings
                .first()
                .or_else(|| snapshot.post_bindings.first())
            {
                let index = plan
                    .operations
                    .iter()
                    .position(|operation| operation.definition_id == binding.resource_id)
                    .unwrap_or(0);
                plan.operations[index].targets.push(PlannedTarget {
                    binding_id: Some(binding.id.clone()),
                    target: binding.target.clone(),
                    binding: binding.clone(),
                    baseline: snapshot
                        .pre_baselines
                        .iter()
                        .find(|(id, _)| id == &binding.id)
                        .map(|(_, baseline)| baseline.clone()),
                    steps: Vec::new(),
                    warnings: vec!["恢复扩展库记录，不修改客户端文件".to_string()],
                    changes: Vec::new(),
                });
            } else {
                return Err(CommandError::new(
                    "extension-not-restorable",
                    "该操作没有可恢复的客户端或库状态",
                ));
            }
        }

        plan.plan_id = new_id("plan");
        plan.created_at = now();
        plan.preconditions.expires_at = chrono::Utc::now()
            .checked_add_signed(chrono::Duration::seconds(
                asb_core::extensions::plan::PLAN_TTL_SECONDS as i64,
            ))
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        plan.preconditions.generation = store.manifest().map_err(store_error)?.generation;
        let restore_commit = LibraryCommit {
            history_operation_id: restore_operation_id.clone(),
            binding_upserts: upserts,
            baseline_files: snapshot.pre_baselines.clone(),
            // A binding delete removes its baseline file implicitly; listing
            // that baseline here as well would make the commit contradictory.
            baseline_deletes: snapshot
                .post_baselines
                .iter()
                .filter(|(post_id, _)| {
                    !snapshot
                        .pre_baselines
                        .iter()
                        .any(|(pre_id, _)| pre_id == post_id)
                        && !deletes.contains(post_id)
                })
                .map(|(id, _)| id.clone())
                .collect(),
            binding_deletes: deletes,
            snapshot: None,
            pre_bindings: store.list_bindings().map_err(store_error)?,
            pre_baselines: current_baseline_files(&store)?,
        };
        let plan_id = plan.plan_id.clone();
        pending_plans().lock().expect("plans").insert(
            plan_id.clone(),
            PendingPlan {
                plan,
                operation_id: restore_operation_id,
                journal_dir,
                commit: restore_commit,
            },
        );
        let view = redact_plan(&pending_plans().lock().expect("plans")[&plan_id].plan);
        Ok(view)
    })
    .await
}

mod invert;
