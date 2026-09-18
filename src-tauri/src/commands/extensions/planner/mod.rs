//! Batch planning: one plan request carries several resource
//! operations; the planner turns each into verifiable steps with
//! expectations, warnings, and the post-apply library state.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::extensions::store::LibraryCommit;
use asb_core::extensions::contracts::{
    DesiredState, DocumentSyntax, ExtensionBinding, ExtensionDefinition, ExtensionPayload,
    ExtensionTarget, PlanOperation, EXTENSIONS_SCHEMA_VERSION,
};
use asb_core::extensions::plan::{
    redact_plan, ExtensionPlan, ExtensionPlanView, PlanStep, PlannedOperation, PlannedTarget,
};
use asb_core::extensions::validate::validate_binding;
use serde::Deserialize;
use tauri::AppHandle;

use self::deploy::required_id;
use self::document::synchronize_document_baselines;
use super::support::*;
use crate::commands::error::{blocking, state, CommandError};
use crate::extensions::discovery::DiscoveredPaths;
use crate::extensions::secrets::SecretBackend;
use crate::extensions::secrets::SystemSecrets;
use crate::extensions::store::ExtensionStore;

// ---------------------------------------------------------------- planning

/// One typed resource operation inside a batch plan request. A
/// single-resource request is a batch of one; there is no separate
/// single-resource shape.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanRequestOperation {
    operation: PlanOperation,
    definition_id: Option<String>,
    binding_id: Option<String>,
    /// Targets for installs.
    #[serde(default)]
    targets: Vec<ExtensionTarget>,
    /// For Claude project skill visibility: shared settings vs personal.
    shared_settings: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanRequest {
    operations: Vec<PlanRequestOperation>,
}

/// Programmable constructor for orchestrators that already know the exact
/// binding-level intent (the Codex project plan apply): one enable/disable
/// operation per binding, in the order given. The request type stays private
/// to this module so no second writer can build plans by hand.
pub(crate) fn binding_state_request(toggles: Vec<(String, bool)>) -> PlanRequest {
    PlanRequest {
        operations: toggles
            .into_iter()
            .map(|(binding_id, enable)| PlanRequestOperation {
                operation: if enable {
                    PlanOperation::Enable
                } else {
                    PlanOperation::Disable
                },
                definition_id: None,
                binding_id: Some(binding_id),
                targets: Vec::new(),
                shared_settings: None,
            })
            .collect(),
    }
}

#[tauri::command]
pub async fn prepare_extension_plan(
    app: AppHandle,
    request: PlanRequest,
) -> Result<ExtensionPlanView, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let paths = DiscoveredPaths::from_env()
            .map_err(|error| CommandError::new("app-state-unavailable", error))?;
        // A recording resolver: renders resolve stored secrets as before,
        // and every resolved reference is hashed into the plan's
        // preconditions so apply refuses a silently changed credential.
        let resolved: std::cell::RefCell<BTreeMap<String, String>> =
            std::cell::RefCell::new(BTreeMap::new());
        let recorder = |reference: &str| -> Option<String> {
            let value = SystemSecrets.get(reference).ok()?;
            resolved.borrow_mut().insert(reference.to_string(), {
                use sha2::Digest;
                sha2::Sha256::digest(value.as_bytes())
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            });
            Some(value)
        };
        let planner = Planner {
            store: &store,
            paths: &paths,
            secrets: &recorder,
        };
        let plan = planner.build(&request)?;
        stage_plan(&store, plan)
    })
    .await
}

/// The one staging path every prepared plan goes through: identities, the
/// validity window, backup wiring, baseline synchronization, the pre-state
/// snapshot, and the pending-plan registration. Apply later consumes
/// exactly this pending plan.
pub(super) fn stage_plan(
    store: &ExtensionStore,
    mut plan: ExtensionPlan,
) -> Result<ExtensionPlanView, CommandError> {
    let operation_id = new_id("op");
    let plan_id = new_id("plan");
    let backup_root = store.backup_root(&operation_id);
    let journal_dir = store.transaction_dir(&operation_id);
    plan.plan_id = plan_id.clone();
    plan.created_at = now();
    // The validity window: now + the plan TTL.
    plan.preconditions.expires_at = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::seconds(
            asb_core::extensions::plan::PLAN_TTL_SECONDS as i64,
        ))
        .unwrap_or_else(chrono::Utc::now)
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    plan.preconditions.generation = store.manifest().map_err(store_error)?.generation;
    // Wire every step's backup directory to this operation's root.
    for target in plan.planned_targets_mut() {
        for step in &mut target.steps {
            match step {
                PlanStep::DocumentWrite { backup_dir, .. }
                | PlanStep::DocumentRemove { backup_dir, .. }
                | PlanStep::DirectoryDeploy { backup_dir, .. }
                | PlanStep::DirectoryRemove { backup_dir, .. } => {
                    *backup_dir = backup_root.to_string_lossy().to_string();
                }
            }
        }
    }
    let removed_bindings: BTreeSet<String> = plan
        .operations
        .iter()
        .filter(|operation| operation.operation == PlanOperation::Remove)
        .flat_map(|operation| operation.targets.iter())
        .filter(|target| target.binding.desired == DesiredState::Disabled)
        .map(|target| target.binding.id.clone())
        .collect();
    let baseline_files = synchronize_document_baselines(store, &plan, &removed_bindings)?;
    // Snapshot every library row the batch can change, including
    // baselines of other bindings that share a document. The executor
    // journals this complete pre-state before any client write.
    let affected_bindings: BTreeSet<String> = plan
        .planned_bindings()
        .map(|binding| binding.id.clone())
        .chain(
            baseline_files
                .iter()
                .map(|(binding_id, _)| binding_id.clone()),
        )
        .chain(removed_bindings.iter().cloned())
        .collect();
    let current_bindings = store.list_bindings().map_err(store_error)?;
    let mut pre_bindings = Vec::new();
    let mut pre_baselines = Vec::new();
    for binding_id in &affected_bindings {
        if let Some(binding) = current_bindings
            .iter()
            .find(|binding| binding.id == *binding_id)
        {
            pre_bindings.push(binding.clone());
        }
        if let Some(baseline) = store.get_baseline_file(binding_id).map_err(store_error)? {
            pre_baselines.push((binding_id.clone(), baseline));
        }
    }
    let commit = LibraryCommit {
        history_operation_id: operation_id.clone(),
        binding_upserts: plan
            .planned_bindings()
            .filter(|binding| !removed_bindings.contains(&binding.id))
            .cloned()
            .collect(),
        baseline_files,
        baseline_deletes: Vec::new(),
        binding_deletes: removed_bindings.into_iter().collect(),
        snapshot: None,
        pre_bindings,
        pre_baselines,
    };
    pending_plans().lock().expect("plans").insert(
        plan_id.clone(),
        PendingPlan {
            plan,
            operation_id,
            journal_dir,
            commit,
        },
    );
    let view = redact_plan(&pending_plans().lock().expect("plans")[&plan_id].plan);
    Ok(view)
}

/// The resolved MCP document of one target: the single owner of "client ×
/// scope → path + syntax + patch surface". Unsupported combinations fail
/// resolution instead of falling back to another client's path.
pub(super) struct McpDocumentTarget {
    path: PathBuf,
    syntax: DocumentSyntax,
    scope: McpScope,
}

pub(super) enum McpScope {
    /// Codex `mcp_servers.<key>` tables (user or project-shared config).
    CodexServers,
    /// Claude top-level `mcpServers.<key>` (user `~/.claude.json` or
    /// project-shared `.mcp.json`).
    ClaudeUserServers,
    /// Claude private `projects.<path>.mcpServers.<key>` in the user doc.
    ClaudeProjectPrivate { project_path: String },
}

pub(super) struct Planner<'a> {
    pub(super) store: &'a ExtensionStore,
    pub(super) paths: &'a DiscoveredPaths,
    pub(super) secrets: &'a dyn Fn(&str) -> Option<String>,
}

impl Planner<'_> {
    fn build(&self, request: &PlanRequest) -> Result<ExtensionPlan, CommandError> {
        if request.operations.is_empty() {
            return Err(CommandError::new(
                "extension-invalid",
                "批量计划没有包含任何资源操作",
            ));
        }
        // Per-document working state for the whole batch: every patch
        // threads through one text per document and each document is
        // written exactly once when the batch is complete.
        // Per-document working state for the whole batch: every patch
        // threads through one text per document and each document is
        // written exactly once when the batch is complete.
        let mut batch = document::DocumentBatch::default();
        let mut operations = Vec::new();
        for entry in &request.operations {
            let planned = match entry.operation {
                PlanOperation::Install => {
                    let definition_id = required_id(&entry.definition_id, "安装计划缺少扩展 id")?;
                    self.build_install(definition_id, &entry.targets, &mut batch)?
                }
                PlanOperation::Update => {
                    let definition_id = required_id(&entry.definition_id, "更新计划缺少扩展 id")?;
                    let definition = self
                        .store
                        .get_definition(definition_id)
                        .map_err(store_error)?
                        .ok_or_else(|| {
                            CommandError::new("extension-not-found", "扩展不存在或已被删除")
                        })?;
                    let bindings: Vec<ExtensionBinding> = self
                        .store
                        .list_bindings()
                        .map_err(store_error)?
                        .into_iter()
                        .filter(|binding| binding.resource_id == definition_id)
                        .collect();
                    self.build_redeploy(&definition, &bindings, &mut batch)?
                }
                PlanOperation::Enable | PlanOperation::Disable | PlanOperation::Remove => {
                    let binding_id = required_id(&entry.binding_id, "该操作缺少绑定 id")?;
                    self.build_binding_change(
                        entry.operation,
                        binding_id,
                        entry.shared_settings,
                        &mut batch,
                    )?
                }
                PlanOperation::Restore => {
                    return Err(CommandError::new(
                        "extension-invalid",
                        "恢复请使用 prepare_extension_restore",
                    ))
                }
                PlanOperation::Repair => {
                    return Err(CommandError::new(
                        "extension-invalid",
                        "修复请使用 prepare_extension_repair",
                    ))
                }
            };
            batch.advance(planned.targets.len());
            operations.push(planned);
        }
        // One resource may appear at most once per batch; overlapping writes
        // to the same definition are a request bug, not a merge.
        let mut seen = BTreeSet::new();
        for operation in &operations {
            if !seen.insert(operation.definition_id.clone()) {
                return Err(CommandError::new(
                    "extension-conflict",
                    "同一批量计划不能对同一扩展声明多个操作",
                ));
            }
        }
        // Every touched document lands as exactly one write, carried by the
        // first target that changed it.
        document::materialize_document_writes(&mut batch, &mut operations);
        Ok(ExtensionPlan {
            plan_id: String::new(),
            created_at: String::new(),
            operations,
            preconditions: asb_core::extensions::plan::PlanPreconditions {
                expires_at: String::new(),
                generation: 0,
                secret_digests: BTreeMap::new(),
            },
        })
    }

    fn build_install(
        &self,
        definition_id: &str,
        targets: &[ExtensionTarget],
        batch: &mut document::DocumentBatch,
    ) -> Result<PlannedOperation, CommandError> {
        if targets.is_empty() {
            return Err(CommandError::new(
                "extension-invalid",
                "安装计划没有选择任何目标",
            ));
        }
        let definition = self
            .store
            .get_definition(definition_id)
            .map_err(store_error)?
            .ok_or_else(|| CommandError::new("extension-not-found", "扩展不存在或已被删除"))?;
        let existing_bindings = self.store.list_bindings().map_err(store_error)?;
        let first_target = batch.targets();
        let mut operation = PlannedOperation {
            definition_id: definition.id.clone(),
            definition_revision: definition.revision,
            operation: PlanOperation::Install,
            targets: Vec::new(),
        };
        for (index, target) in targets.iter().enumerate() {
            if targets[..index].iter().any(|previous| previous == target) {
                return Err(CommandError::new(
                    "extension-conflict",
                    "同一安装计划不能重复选择相同的客户端目标",
                ));
            }
            if existing_bindings
                .iter()
                .any(|binding| binding.resource_id == definition.id && &binding.target == target)
            {
                return Err(CommandError::new(
                    "extension-conflict",
                    "该扩展已绑定到所选目标；请使用更新、启用、停用或移除操作",
                ));
            }
            let binding = self.new_binding(&definition, target)?;
            validate_binding(&definition, &binding)
                .map_err(|error| CommandError::new("extension-invalid", error.message))?;
            self.push_target(&mut operation, &definition, &binding, batch, first_target + index)?;
        }
        Ok(operation)
    }

    fn push_target(
        &self,
        operation: &mut PlannedOperation,
        definition: &ExtensionDefinition,
        binding: &ExtensionBinding,
        batch: &mut document::DocumentBatch,
        writer: usize,
    ) -> Result<(), CommandError> {
        self.require_write_capabilities(definition, binding, operation.operation)?;
        // Two managed bindings may never fight over one entry position;
        // installs of a fresh binding claim their key against the library
        // just like renames and takeovers do.
        if matches!(definition.payload, ExtensionPayload::Mcp(_)) {
            self.assert_native_key_free(binding)?;
        }
        let (steps, changes, baseline, warnings, adopts_native_entry) =
            self.deploy_steps(definition, binding, batch, writer)?;
        operation.targets.push(PlannedTarget {
            binding_id: Some(binding.id.clone()),
            target: binding.target.clone(),
            binding: binding.clone(),
            baseline,
            steps,
            warnings,
            changes,
            adopts_native_entry,
        });
        Ok(())
    }

    fn new_binding(
        &self,
        definition: &ExtensionDefinition,
        target: &ExtensionTarget,
    ) -> Result<ExtensionBinding, CommandError> {
        let binding_id = new_id("bind");
        match &definition.payload {
            ExtensionPayload::Skill(skill) => {
                let deploy_name = asb_core::extensions::skill::deploy_name_for(&skill.manifest)
                    .map_err(|error| CommandError::new("extension-invalid", error.message))?;
                Ok(ExtensionBinding {
                    schema_version: EXTENSIONS_SCHEMA_VERSION,
                    id: binding_id,
                    resource_id: definition.id.clone(),
                    target: target.clone(),
                    native_key: None,
                    deploy_name: Some(deploy_name),
                    desired: DesiredState::Enabled,
                    locked_digest: None,
                    last_applied_revision: None,
                    updated_at: now(),
                })
            }
            ExtensionPayload::Mcp(_) => Ok(ExtensionBinding {
                schema_version: EXTENSIONS_SCHEMA_VERSION,
                id: binding_id,
                resource_id: definition.id.clone(),
                target: target.clone(),
                native_key: Some(definition.name.clone()),
                deploy_name: None,
                desired: DesiredState::Enabled,
                locked_digest: None,
                last_applied_revision: None,
                updated_at: now(),
            }),
        }
    }
}
mod capability;
mod deploy;
mod document;
mod lifecycle;
mod remove;
mod rename;
mod repair;
mod rules;

pub(super) use repair::build_repair_operations;
