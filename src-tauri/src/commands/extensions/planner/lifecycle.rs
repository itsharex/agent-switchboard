//! Binding lifecycle planning: the dispatcher that turns enable, disable,
//! and remove intents into verified step lists.

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    DesiredState, ExtensionPayload, ExtensionTarget, PlanOperation,
};
use asb_core::extensions::plan::{PlannedOperation, PlannedTarget};
use asb_core::extensions::validate::validate_binding;

use crate::commands::error::CommandError;
use crate::commands::extensions::support::*;

use super::Planner;

impl Planner<'_> {
    pub(super) fn build_binding_change(
        &self,
        operation: PlanOperation,
        binding_id: &str,
        shared_settings: Option<bool>,
        batch: &mut super::document::DocumentBatch,
    ) -> Result<PlannedOperation, CommandError> {
        let binding = self
            .store
            .list_bindings()
            .map_err(store_error)?
            .into_iter()
            .find(|binding| binding.id == binding_id)
            .ok_or_else(|| {
                CommandError::keyed(
                    "extension-not-found",
                    "errors.extops.bindingNotFound",
                    "绑定不存在或已被移除",
                )
            })?;
        let definition = self
            .store
            .get_definition(&binding.resource_id)
            .map_err(store_error)?
            .ok_or_else(|| {
                CommandError::keyed(
                    "extension-not-found",
                    "errors.extops.definitionRecordMissing",
                    "扩展定义不存在",
                )
            })?;
        validate_binding(&definition, &binding)
            .map_err(|error| CommandError::new("extension-invalid", error.message))?;
        self.require_write_capabilities(&definition, &binding, operation)?;
        let writer = batch.targets();
        let mut plan = PlannedOperation {
            definition_id: definition.id.clone(),
            definition_revision: definition.revision,
            operation,
            targets: Vec::new(),
        };
        match operation {
            PlanOperation::Enable => {
                let mut next = binding.clone();
                next.desired = DesiredState::Enabled;
                next.updated_at = now();
                // A Claude private-project MCP has two owned positions in
                // one user document: the server and its project disable
                // member. They must thread into one document state.
                let (steps, changes, mut baseline, mut warnings, adopts_native_entry) =
                    if matches!(
                        (&definition.payload, &next.target),
                        (
                            ExtensionPayload::Mcp(_),
                            ExtensionTarget::ProjectPrivate {
                                client: AppKind::Claude,
                                ..
                            }
                        )
                    ) {
                        // A private re-enable requires a current baseline and
                        // never adopts a foreign entry; its document write is
                        // materialized with the batch.
                        let (changes, baseline, warnings) =
                            self.enable_private_mcp_steps(&definition, &next, batch, writer)?;
                        (Vec::new(), changes, baseline, warnings, false)
                    } else {
                        // Skill targets carry their directory deploy here;
                        // MCP document writes are materialized with the batch.
                        self.deploy_steps(&definition, &next, batch, writer)?
                    };
                let (cleared_baseline, mut clear_warnings) =
                    self.clear_disable_steps(&definition, &binding, baseline.as_ref(), batch, writer)?;
                if let Some(cleared_baseline) = cleared_baseline {
                    baseline = Some(cleared_baseline);
                }
                warnings.append(&mut clear_warnings);
                plan.targets.push(PlannedTarget {
                    binding_id: Some(binding.id.clone()),
                    target: next.target.clone(),
                    binding: next,
                    baseline,
                    steps,
                    warnings,
                    changes,
                    adopts_native_entry,
                });
            }
            PlanOperation::Disable => {
                let (changes, new_baseline, warnings) =
                    self.disable_steps(&definition, &binding, shared_settings, batch, writer)?;
                // The committed state comes from the verified plan: the
                // binding lands in the library as Disabled.
                let mut next = binding.clone();
                next.desired = DesiredState::Disabled;
                next.updated_at = now();
                plan.targets.push(PlannedTarget {
                    binding_id: Some(binding.id.clone()),
                    target: binding.target.clone(),
                    binding: next,
                    baseline: new_baseline,
                    steps: Vec::new(),
                    warnings,
                    changes,
                    adopts_native_entry: false,
                });
            }
            PlanOperation::Remove => {
                let (changes, mut new_baseline, mut warnings) =
                    self.remove_document_steps(&definition, &binding, batch, writer)?;
                let mut steps = Vec::new();
                if let ExtensionPayload::Skill(_) = &definition.payload {
                    let existing_baseline = self
                        .store
                        .get_baseline_file(&binding.id)
                        .map_err(store_error)?;
                    let (cleared_baseline, mut rule_warnings) = self.clear_disable_steps(
                        &definition,
                        &binding,
                        existing_baseline.as_ref(),
                        batch,
                        writer,
                    )?;
                    let _ = cleared_baseline;
                    warnings.append(&mut rule_warnings);
                    if let Some((remove_step, restores_original)) =
                        self.skill_remove_step(&definition, &binding)?
                    {
                        steps.push(remove_step);
                        new_baseline = None;
                        warnings.push(
                            if restores_original {
                                "将恢复接管该目录时的原始内容"
                            } else {
                                "将移除该绑定部署的目录内容"
                            }
                            .to_string(),
                        );
                    }
                }
                let mut next = binding.clone();
                next.desired = DesiredState::Disabled;
                next.updated_at = now();
                plan.targets.push(PlannedTarget {
                    binding_id: Some(binding.id.clone()),
                    target: binding.target.clone(),
                    binding: next,
                    // A removed binding keeps no baseline; restore replays
                    // the operation snapshot, not current baselines.
                    baseline: None,
                    steps,
                    warnings,
                    changes,
                    adopts_native_entry: false,
                });
                let _ = new_baseline;
            }
            _ => {
                return Err(CommandError::keyed(
                    "extension-invalid",
                    "errors.extops.unsupportedOperation",
                    "不支持的操作",
                ))
            }
        }
        Ok(plan)
    }
}
