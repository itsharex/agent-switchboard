//! Redeploy and rename planning: moving a binding to a new native key
//! inside one document step, with ownership checks that keep renames
//! collision-free.

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    DesiredState, ExtensionBinding, ExtensionDefinition, ExtensionPayload, ManagedBaseline,
    ManagedBaselineFile, PlanOperation,
};
use asb_core::extensions::mcp::{
    apply_claude_project_disabled_members, apply_claude_project_private_server_patches,
    apply_claude_user_server_patches, apply_codex_server_patches,
    claude_project_disabled_member_present, render_claude, render_codex, ClaudeHost,
};
use asb_core::extensions::plan::{PlanStep, PlannedOperation, PlannedTarget};

use crate::commands::error::CommandError;
use crate::commands::extensions::support::*;

use super::deploy::merge_baseline_file_entry;
use super::document::{document_hash_of, json_or_toml_step, read_document, same_mcp_namespace};
use super::{McpScope, Planner};

impl Planner<'_> {
    pub(super) fn build_redeploy(
        &self,
        definition: &ExtensionDefinition,
        bindings: &[ExtensionBinding],
    ) -> Result<PlannedOperation, CommandError> {
        let mut plan = PlannedOperation {
            definition_id: definition.id.clone(),
            definition_revision: definition.revision,
            operation: PlanOperation::Update,
            targets: Vec::new(),
        };
        for binding in bindings {
            // A renamed MCP key moves every binding — enabled and disabled
            // alike — inside this one plan; a skipped disabled binding would
            // later re-apply the stale key.
            let rename_from = match (&definition.payload, binding.native_key.as_deref()) {
                (ExtensionPayload::Mcp(_), Some(current_key)) if current_key != definition.name => {
                    Some(current_key.to_string())
                }
                _ => None,
            };
            if binding.desired != DesiredState::Enabled && rename_from.is_none() {
                continue;
            }
            // An enabled skill binding pinned to an older content version
            // stays on its locked version: the plan reports the skip instead
            // of silently moving the target to content the user froze.
            if let (ExtensionPayload::Skill(skill), Some(locked)) =
                (&definition.payload, binding.locked_digest.as_deref())
            {
                if locked != skill.content_digest {
                    plan.targets.push(PlannedTarget {
                        binding_id: Some(binding.id.clone()),
                        target: binding.target.clone(),
                        binding: binding.clone(),
                        baseline: None,
                        steps: Vec::new(),
                        warnings: vec![format!(
                            "该目标已固定版本 {}；解除固定后才会更新",
                            locked.get(..12).unwrap_or(locked)
                        )],
                        changes: Vec::new(),
                    });
                    continue;
                }
            }
            if let Some(old_key) = rename_from {
                let mut next = binding.clone();
                next.native_key = Some(definition.name.clone());
                next.updated_at = now();
                self.push_rename_target(&mut plan, definition, &next, &old_key)?;
            } else {
                self.push_target(&mut plan, definition, binding)?;
            }
        }
        Ok(plan)
    }

    /// One MCP server-key rename: the binding lands with the new native key
    /// while the same document step removes the old entry and writes the new
    /// one, so no apply can leave both keys behind.
    pub(super) fn push_rename_target(
        &self,
        operation: &mut PlannedOperation,
        definition: &ExtensionDefinition,
        next: &ExtensionBinding,
        old_key: &str,
    ) -> Result<(), CommandError> {
        self.require_write_capabilities(definition, next, operation.operation)?;
        self.assert_native_key_free(next)?;
        let (steps, changes, baseline, warnings) =
            self.rename_deploy_steps(definition, next, old_key)?;
        operation.targets.push(PlannedTarget {
            binding_id: Some(next.id.clone()),
            target: next.target.clone(),
            binding: next.clone(),
            baseline,
            steps,
            warnings,
            changes,
        });
        Ok(())
    }

    /// Rejects a key another binding already occupies in the same document
    /// and namespace; the document-level before-check cannot see a key that
    /// has not been written yet.
    pub(crate) fn assert_native_key_free(
        &self,
        binding: &ExtensionBinding,
    ) -> Result<(), CommandError> {
        let new_key = binding
            .native_key
            .as_deref()
            .ok_or_else(|| CommandError::new("extension-invalid", "MCP 绑定缺少服务键"))?;
        let own = self.mcp_document(binding)?;
        for other in self.store.list_bindings().map_err(store_error)? {
            if other.id == binding.id || other.native_key.as_deref() != Some(new_key) {
                continue;
            }
            let Ok(other_target) = self.mcp_document(&other) else {
                continue;
            };
            if other_target.path == own.path && same_mcp_namespace(&other_target.scope, &own.scope)
            {
                return Err(CommandError::new(
                    "extension-conflict",
                    format!("服务键 {new_key} 已被同一配置文档中的另一绑定占用，请换一个键"),
                ));
            }
        }
        Ok(())
    }

    /// Deploys one binding under its new native key: the old entry is
    /// removed and the new one written in a single document step, a private
    /// project's disable member moves with the key, and the baseline drops
    /// the old positions while recording the new one.
    #[allow(clippy::type_complexity)]
    pub(super) fn rename_deploy_steps(
        &self,
        definition: &ExtensionDefinition,
        binding: &ExtensionBinding,
        old_key: &str,
    ) -> Result<
        (
            Vec<PlanStep>,
            Vec<asb_core::extensions::mcp::EntryChange>,
            Option<ManagedBaselineFile>,
            Vec<String>,
        ),
        CommandError,
    > {
        let ExtensionPayload::Mcp(mcp) = &definition.payload else {
            return Err(CommandError::new(
                "extension-invalid",
                "重命名只适用于 MCP 定义",
            ));
        };
        let new_key = binding
            .native_key
            .as_deref()
            .ok_or_else(|| CommandError::new("extension-invalid", "MCP 绑定缺少服务键"))?;
        let target = self.mcp_document(binding)?;
        let document = target.path.clone();
        let doc_str = document.to_string_lossy().to_string();
        let syntax = target.syntax;
        let client = binding.target.client();
        let current = read_document(&document)?;
        let existed = current.is_some();
        let text = current.clone().unwrap_or(match client {
            AppKind::Codex => String::new(),
            AppKind::Claude => "{}".to_string(),
        });
        let enabled = binding.desired == DesiredState::Enabled;
        // External-change guard: a baseline that no longer matches the
        // document fails here instead of silently moving the key.
        let _ = self.document_baseline_is_current(binding, &document)?;

        let member_collection = match &target.scope {
            McpScope::ClaudeProjectPrivate { project_path } => {
                Some(format!("projects.{project_path}.disabledMcpServers"))
            }
            _ => None,
        };
        let server_pointer = |key: &str| match &target.scope {
            McpScope::CodexServers => format!("mcp_servers.{key}"),
            McpScope::ClaudeUserServers => format!("mcpServers.{key}"),
            McpScope::ClaudeProjectPrivate { project_path } => {
                format!("projects.{project_path}.mcpServers.{key}")
            }
        };
        let old_pointer = server_pointer(old_key);
        let new_pointer = server_pointer(new_key);

        let (rendered, changes) = match &target.scope {
            McpScope::CodexServers => {
                let render = render_codex(mcp, enabled, self.secrets).map_err(projection_error)?;
                apply_codex_server_patches(
                    &text,
                    &[
                        (old_key.to_string(), None),
                        (new_key.to_string(), Some(render)),
                    ],
                )
                .map_err(adapter_error)?
            }
            McpScope::ClaudeUserServers => {
                let render = enabled
                    .then(|| render_claude(mcp, self.secrets, ClaudeHost::current()))
                    .transpose()
                    .map_err(projection_error)?;
                apply_claude_user_server_patches(
                    &text,
                    &[(old_key.to_string(), None), (new_key.to_string(), render)],
                )
                .map_err(adapter_error)?
            }
            McpScope::ClaudeProjectPrivate { project_path } => {
                let render = enabled
                    .then(|| render_claude(mcp, self.secrets, ClaudeHost::current()))
                    .transpose()
                    .map_err(projection_error)?;
                let (after_servers, mut server_changes) =
                    apply_claude_project_private_server_patches(
                        &text,
                        project_path,
                        &[(old_key.to_string(), None), (new_key.to_string(), render)],
                    )
                    .map_err(adapter_error)?;
                let add_members: Vec<String> = if enabled {
                    Vec::new()
                } else {
                    vec![new_key.to_string()]
                };
                let (rendered, member_changes) = apply_claude_project_disabled_members(
                    &after_servers,
                    project_path,
                    &add_members,
                    &[old_key.to_string()],
                )
                .map_err(adapter_error)?;
                server_changes.extend(member_changes);
                (rendered, server_changes)
            }
        };
        let changes = changes;
        // The new key's position never belonged to this binding (renames
        // only move between keys), so any pre-existing native value there —
        // tracked or not — must block the rename.
        if let Some(change) = changes
            .iter()
            .find(|change| change.pointer == new_pointer && change.after.is_some())
        {
            if change.before.is_some() {
                return Err(CommandError::new(
                    "extension-conflict",
                    format!("{doc_str} 已有同名原生 MCP 服务，不能重命名到该键"),
                ));
            }
        }

        // Settle the baseline: the old key's positions stop being owned; the
        // new positions are recorded with this plan's post-state. MCP
        // entries never carry a takeover original (install refuses same-name
        // entries), so dropping the old entries loses no restore material.
        let mut baseline = self
            .store
            .get_baseline_file(&binding.id)
            .map_err(store_error)?
            .unwrap_or_else(|| ManagedBaselineFile::new(&binding.id, Vec::new()));
        baseline.entries.retain(|entry| match entry {
            ManagedBaseline::DocumentEntry {
                target_path,
                entry_pointer,
                ..
            } => {
                !(target_path == &doc_str
                    && (entry_pointer == &old_pointer || entry_pointer == &new_pointer))
            }
            ManagedBaseline::SetMember {
                target_path,
                collection_pointer,
                member,
                ..
            } => {
                !(target_path == &doc_str
                    && Some(collection_pointer.as_str()) == member_collection.as_deref()
                    && (member == old_key || member == new_key))
            }
            ManagedBaseline::Directory { .. } => true,
        });
        let new_change = changes.iter().find(|change| change.pointer == new_pointer);
        let document_hash = if changes.is_empty() {
            document_hash_of(&document)?.unwrap_or_default()
        } else {
            sha_hex(rendered.as_bytes())
        };
        baseline = merge_baseline_file_entry(
            baseline,
            ManagedBaseline::DocumentEntry {
                target_path: doc_str.clone(),
                entry_pointer: new_pointer,
                original_value: new_change.and_then(|change| change.before.clone()),
                last_written_value: new_change.and_then(|change| change.after.clone()),
                last_document_hash: document_hash.clone(),
                target_existed_before: existed,
                backup_reference: None,
            },
        );
        if let (Some(collection), McpScope::ClaudeProjectPrivate { project_path }) =
            (&member_collection, &target.scope)
        {
            if !enabled {
                let original_present =
                    claude_project_disabled_member_present(&text, project_path, new_key)
                        .map_err(adapter_error)?;
                baseline = merge_baseline_file_entry(
                    baseline,
                    ManagedBaseline::SetMember {
                        target_path: doc_str,
                        collection_pointer: collection.clone(),
                        member: new_key.to_string(),
                        original_present,
                        last_present: true,
                        last_document_hash: document_hash,
                        target_existed_before: existed,
                        backup_reference: None,
                    },
                );
            }
        }

        let steps = if changes.is_empty() {
            Vec::new()
        } else {
            vec![json_or_toml_step(client, &document, rendered, syntax)?]
        };
        Ok((
            steps,
            changes,
            Some(baseline),
            vec![format!(
                "服务键由 {old_key} 改为 {new_key}；将在同一计划中移除旧条目并写入新键"
            )],
        ))
    }
}

#[cfg(test)]
mod tests;
