//! Extension plans: the executable payloads held by the backend and the
//! redacted views the frontend receives.
//!
//! A plan is prepared against the current disk state and becomes the sole
//! authorization for one apply. The executor re-verifies every expectation
//! (hashes, existence, digests) before writing; a mismatch yields
//! [`PlanStale`] and the user previews again. Plan payloads contain rendered
//! documents with resolved secrets and are never serialized to the frontend;
//! views are built through the redaction helpers in this module.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::contracts::AppKind;
use crate::extensions::contracts::{
    DocumentSyntax, ExtensionBinding, ExtensionTarget, ManagedBaselineFile, PlanOperation,
};
use crate::extensions::mcp::EntryChange;

/// How long a prepared plan may be applied after creation.
pub const PLAN_TTL_SECONDS: u64 = 15 * 60;

/// The library and credential state a plan was prepared against. Apply
/// re-verifies every entry after taking the locks and before any write;
/// a mismatch yields [`PlanStale`] and the user previews again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanPreconditions {
    /// RFC 3339 UTC time after which the plan must be re-prepared.
    pub expires_at: String,
    /// Library generation at preparation time.
    pub generation: u64,
    /// Digest of every resolved secret reference used by this plan, keyed
    /// by reference. Apply re-resolves and re-hashes; a changed value
    /// invalidates the previewed render.
    pub secret_digests: BTreeMap<String, String>,
}

/// The full backend-owned plan for one batch extension operation. The plan
/// is the only write authorization and directly carries the post-apply
/// library state; there is no second pending-commit source of truth. A
/// batch holds one or more typed resource operations — a single-resource
/// request is just a batch of one, never a separate shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtensionPlan {
    pub plan_id: String,
    /// RFC 3339 UTC creation time.
    pub created_at: String,
    pub operations: Vec<PlannedOperation>,
    pub preconditions: PlanPreconditions,
}

/// One resource's operations inside a batch: every target the resource is
/// applied to, with the definition revision the plan was prepared against.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedOperation {
    pub definition_id: String,
    pub definition_revision: u64,
    pub operation: PlanOperation,
    pub targets: Vec<PlannedTarget>,
}

impl ExtensionPlan {
    /// All planned targets in batch order. The executor and the library
    /// commit consume exactly this sequence.
    pub fn planned_targets(&self) -> impl Iterator<Item = &PlannedTarget> {
        self.operations
            .iter()
            .flat_map(|operation| operation.targets.iter())
    }

    /// Mutable variant used by preparation to wire step metadata.
    pub fn planned_targets_mut(&mut self) -> impl Iterator<Item = &mut PlannedTarget> {
        self.operations
            .iter_mut()
            .flat_map(|operation| operation.targets.iter_mut())
    }

    /// All bindings in their post-apply state, in target order. The library
    /// commit consumes exactly this list.
    pub fn planned_bindings(&self) -> impl Iterator<Item = &ExtensionBinding> {
        self.planned_targets().map(|target| &target.binding)
    }

    /// Post-apply baseline files keyed by binding id.
    pub fn planned_baselines(&self) -> Vec<(String, &ManagedBaselineFile)> {
        self.planned_targets()
            .filter_map(|target| {
                target
                    .baseline
                    .as_ref()
                    .map(|baseline| (target.binding.id.clone(), baseline))
            })
            .collect()
    }

    /// The dominant user-facing label of the batch, for records where a
    /// single verb must summarize mixed per-resource operations.
    pub fn summary_operation(&self) -> PlanOperation {
        // Install and Remove dominate; otherwise the first operation speaks.
        for operation in &self.operations {
            if matches!(
                operation.operation,
                PlanOperation::Install | PlanOperation::Remove
            ) {
                return operation.operation;
            }
        }
        self.operations
            .first()
            .map(|operation| operation.operation)
            .unwrap_or(PlanOperation::Update)
    }
}

/// One target's planned end state, executable steps, and preview material.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedTarget {
    /// Existing binding id, absent for a brand-new binding.
    pub binding_id: Option<String>,
    pub target: ExtensionTarget,
    /// The binding as it should exist after a successful apply.
    pub binding: ExtensionBinding,
    /// The complete baseline file as it should exist after a successful
    /// apply (directory ownership plus every rule this binding owns).
    pub baseline: Option<ManagedBaselineFile>,
    pub steps: Vec<PlanStep>,
    pub warnings: Vec<String>,
    /// Raw entry changes for preview and baseline bookkeeping.
    pub changes: Vec<EntryChange>,
}

/// The finite set of executable step shapes. There is deliberately no
/// generic "run command" step: the executor can only do exactly these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PlanStep {
    /// Write one prepared document after verifying the current content
    /// still matches the preparation.
    DocumentWrite {
        client: AppKind,
        path: String,
        /// Digest of the document as prepared; `None` means the file must
        /// not exist yet.
        expected_content_hash: Option<String>,
        expected_existed: bool,
        rendered: String,
        syntax: DocumentSyntax,
        backup_dir: String,
    },
    /// Remove a document that was created by the operation being restored.
    /// The original content is backed up before removal so this inverse is
    /// itself fully reversible.
    DocumentRemove {
        client: AppKind,
        path: String,
        expected_content_hash: String,
        syntax: DocumentSyntax,
        backup_dir: String,
    },
    /// Deploy one managed skill directory from a prepared staging copy.
    DirectoryDeploy {
        client: AppKind,
        target_dir: String,
        staging_dir: String,
        files: Vec<PlannedFile>,
        /// Digest of the currently deployed directory, when one exists.
        original_digest: Option<String>,
        backup_dir: String,
    },
    /// Remove a managed skill directory after ownership verification.
    DirectoryRemove {
        client: AppKind,
        target_dir: String,
        expected_files: Vec<PlannedFile>,
        expected_digest: String,
        backup_dir: String,
    },
}

/// One file inside a managed directory step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlannedFile {
    /// `/`-separated path relative to the directory root.
    pub relative_path: String,
    pub digest: String,
    pub mode: u32,
    pub size: u64,
}

/// Why a plan cannot be applied. The user must re-preview; the backend
/// never silently recalculates and writes.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct PlanStale {
    pub message: String,
}

/// Redacted plan projection returned to the frontend, grouped by resource.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionPlanView {
    pub plan_id: String,
    pub created_at: String,
    pub expires_at: String,
    pub operations: Vec<PlannedOperationView>,
}

/// One resource's redacted slice of the batch preview.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedOperationView {
    pub definition_id: String,
    pub definition_revision: u64,
    pub operation: PlanOperation,
    pub targets: Vec<PlannedTargetView>,
}

/// One target's redacted preview.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedTargetView {
    pub target: ExtensionTarget,
    pub warnings: Vec<String>,
    pub changes: Vec<PlanChangeView>,
    /// Files the operation deploys or removes, relative to the directory.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileChangeView>,
    /// Whether applying this target writes redacted endpoint, credential, or
    /// argument material into the client document.
    pub writes_sensitive_connection_data: bool,
}

/// One redacted document change.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanChangeView {
    pub pointer: String,
    /// Absent when the entry is created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Absent when the entry is removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
}

/// One managed file's role in an operation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangeView {
    pub relative_path: String,
    pub action: FileAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileAction {
    Deploy,
    Remove,
}

/// Builds the redacted view of one plan, grouped by resource operation.
/// Document changes are masked through the shared redaction helpers;
/// nothing in the view contains a resolved secret value or a staging
/// payload path.
pub fn redact_plan(plan: &ExtensionPlan) -> ExtensionPlanView {
    ExtensionPlanView {
        plan_id: plan.plan_id.clone(),
        created_at: plan.created_at.clone(),
        expires_at: plan.preconditions.expires_at.clone(),
        operations: plan
            .operations
            .iter()
            .map(|operation| PlannedOperationView {
                definition_id: operation.definition_id.clone(),
                definition_revision: operation.definition_revision,
                operation: operation.operation,
                targets: operation.targets.iter().map(redact_target).collect(),
            })
            .collect(),
    }
}

fn redact_target(target: &PlannedTarget) -> PlannedTargetView {
    let mut writes_sensitive_connection_data = false;
    let mut changes = Vec::new();
    for change in &target.changes {
        let (before, after) = crate::extensions::mcp::redact_change(change);
        changes.push(PlanChangeView {
            pointer: change.pointer.clone(),
            before,
            after,
        });
    }
    let mut files = Vec::new();
    for step in &target.steps {
        match step {
            PlanStep::DocumentWrite { rendered, .. } => {
                if crate::extensions::mcp::redact_rendered_entry(rendered)
                    .contains(crate::redact::REDACTED)
                {
                    writes_sensitive_connection_data = true;
                }
            }
            PlanStep::DocumentRemove { .. } => {}
            PlanStep::DirectoryDeploy {
                files: deployed, ..
            } => {
                for file in deployed {
                    files.push(FileChangeView {
                        relative_path: file.relative_path.clone(),
                        action: FileAction::Deploy,
                    });
                }
            }
            PlanStep::DirectoryRemove { expected_files, .. } => {
                for file in expected_files {
                    files.push(FileChangeView {
                        relative_path: file.relative_path.clone(),
                        action: FileAction::Remove,
                    });
                }
            }
        }
    }
    PlannedTargetView {
        target: target.target.clone(),
        warnings: target.warnings.clone(),
        changes,
        files,
        writes_sensitive_connection_data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::contracts::DesiredState;
    use crate::extensions::mcp::apply_codex_server_patches;
    use crate::extensions::mcp::CodexServerRender;
    use std::collections::BTreeMap;

    fn planned_target_with_secret() -> PlannedTarget {
        let render = CodexServerRender::Stdio {
            command: "npx".to_string(),
            args: Vec::new(),
            env: BTreeMap::from([("TOKEN".to_string(), "sk-live-0123456789abcdef".to_string())]),
            env_vars: Vec::new(),
            options: None,
            enabled: true,
        };
        let (rendered, changes) =
            apply_codex_server_patches("", &[("docs".to_string(), Some(render))]).unwrap();
        PlannedTarget {
            binding_id: None,
            target: ExtensionTarget::App {
                client: AppKind::Codex,
            },
            binding: ExtensionBinding {
                schema_version: crate::extensions::contracts::EXTENSIONS_SCHEMA_VERSION,
                id: "b-1".to_string(),
                resource_id: "r-1".to_string(),
                target: ExtensionTarget::App {
                    client: AppKind::Codex,
                },
                native_key: Some("docs".to_string()),
                deploy_name: None,
                desired: DesiredState::Enabled,
                locked_digest: None,
                last_applied_revision: None,
                updated_at: "2026-09-06T00:00:00Z".to_string(),
            },
            baseline: None,
            steps: vec![PlanStep::DocumentWrite {
                client: AppKind::Codex,
                path: "config.toml".to_string(),
                expected_content_hash: None,
                expected_existed: false,
                rendered,
                syntax: crate::extensions::contracts::DocumentSyntax::Toml,
                backup_dir: "backups".to_string(),
            }],
            warnings: Vec::new(),
            changes,
        }
    }

    #[test]
    fn views_redact_secret_shaped_values_and_payloads() {
        let plan = ExtensionPlan {
            plan_id: "plan-1".to_string(),
            created_at: "2026-09-06T00:00:00Z".to_string(),
            operations: vec![PlannedOperation {
                definition_id: "r-1".to_string(),
                definition_revision: 3,
                operation: PlanOperation::Install,
                targets: vec![planned_target_with_secret()],
            }],
            preconditions: PlanPreconditions {
                expires_at: "2026-09-06T00:15:00Z".to_string(),
                generation: 0,
                secret_digests: BTreeMap::new(),
            },
        };
        let view = redact_plan(&plan);
        let target = &view.operations[0].targets[0];
        assert!(target.writes_sensitive_connection_data);
        let after = target.changes[0].after.as_deref().unwrap();
        assert!(after.contains(crate::redact::REDACTED));
        assert!(!after.contains("sk-live"));
        // The view round-trips as JSON without any payload material.
        let json = serde_json::to_string(&view).unwrap();
        assert!(!json.contains("sk-live"));
        assert!(!json.contains("backup_dir"));
    }

    #[test]
    fn plan_payload_steps_survive_serialization_for_the_executor_boundary() {
        let step = PlanStep::DocumentWrite {
            client: AppKind::Codex,
            path: "config.toml".to_string(),
            expected_content_hash: Some("abc".to_string()),
            expected_existed: true,
            rendered: "x = 1".to_string(),
            syntax: DocumentSyntax::Toml,
            backup_dir: "backups".to_string(),
        };
        let json = serde_json::to_string(&step).unwrap();
        let parsed: PlanStep = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, step);
        // Unknown step kinds are rejected rather than ignored.
        assert!(
            serde_json::from_str::<PlanStep>(r#"{"kind":"runCommand","command":"rm"}"#).is_err()
        );
    }
}
