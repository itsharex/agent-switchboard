//! Enable/disable rule bookkeeping: the Codex skills.config entries and
//! Claude skillOverrides members that accompany a binding's desired
//! state, restored from their baselines when the intent flips.

use std::collections::BTreeSet;
use std::path::PathBuf;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    DocumentSyntax, ExtensionBinding, ExtensionDefinition, ExtensionPayload, ManagedBaseline,
    ManagedBaselineFile,
};
use asb_core::extensions::mcp::{
    apply_claude_project_disabled_members, apply_claude_project_private_server_patches,
    apply_claude_skill_override_restore, apply_codex_skill_rule_restore,
    claude_project_disabled_member_present, render_claude, ClaudeHost,
};

use crate::commands::error::CommandError;
use crate::commands::extensions::support::*;

use super::deploy::merge_baseline_file_entry;
use super::{McpScope, Planner};

impl Planner<'_> {
    /// Restores the skill disable rules a previous disable wrote, once the
    /// intent flips back to enabled. The restorations thread into the
    /// batch's per-document working text like every other patch.
    pub(super) fn clear_disable_steps(
        &self,
        definition: &ExtensionDefinition,
        binding: &ExtensionBinding,
        baseline: Option<&ManagedBaselineFile>,
        batch: &mut super::document::DocumentBatch,
        writer: usize,
    ) -> Result<(Option<ManagedBaselineFile>, Vec<String>), CommandError> {
        let ExtensionPayload::Skill(_) = &definition.payload else {
            return Ok((None, Vec::new()));
        };
        let Some(baseline) = baseline else {
            return Ok((None, Vec::new()));
        };
        let client = binding.target.client();
        let syntax = match client {
            AppKind::Codex => DocumentSyntax::Toml,
            AppKind::Claude => DocumentSyntax::Json,
        };
        let prefix = match client {
            AppKind::Codex => "skills.config[path=",
            AppKind::Claude => "skillOverrides.",
        };
        let entries: Vec<(String, String, Option<String>, String)> = baseline
            .entries
            .iter()
            .filter_map(|entry| match entry {
                ManagedBaseline::DocumentEntry {
                    target_path,
                    entry_pointer,
                    original_value,
                    last_written_value: Some(_),
                    last_document_hash,
                    ..
                } if entry_pointer.starts_with(prefix) => Some((
                    target_path.clone(),
                    entry_pointer.clone(),
                    original_value.clone(),
                    last_document_hash.clone(),
                )),
                ManagedBaseline::DocumentEntry { .. }
                | ManagedBaseline::SetMember { .. }
                | ManagedBaseline::Directory { .. } => None,
            })
            .collect();
        if entries.is_empty() {
            return Ok((None, Vec::new()));
        }

        let mut warnings = Vec::new();
        let mut restored_slots: BTreeSet<(String, String)> = BTreeSet::new();

        for (target_path, entry_pointer, original_value, last_document_hash) in entries {
            let document = PathBuf::from(&target_path);
            let work = self.document_work(batch, &document, client, syntax)?;
            let Some(disk_hash) = work.expected_content_hash.clone() else {
                warnings.push(format!(
                    "无法读取 {}；为避免覆盖外部配置，未恢复原始 Skill 停用规则",
                    document.display()
                ));
                continue;
            };
            if disk_hash != last_document_hash {
                warnings.push(format!(
                    "{} 已在本应用上次写入后发生外部变更，未恢复原始 Skill 停用规则",
                    document.display()
                ));
                continue;
            }
            let base = work.rendered.clone();
            let (rendered, restored) = match client {
                AppKind::Codex => {
                    let Some(rule_path) = entry_pointer
                        .strip_prefix("skills.config[path=")
                        .and_then(|pointer| pointer.strip_suffix(']'))
                    else {
                        return Err(CommandError::new(
                            "extension-baseline",
                            "Codex Skill 基线规则指针无效",
                        ));
                    };
                    let (rendered, changes) =
                        apply_codex_skill_rule_restore(&base, rule_path, original_value.as_deref())
                            .map_err(adapter_error)?;
                    (rendered, !changes.is_empty())
                }
                AppKind::Claude => {
                    let Some(name) = entry_pointer.strip_prefix("skillOverrides.") else {
                        return Err(CommandError::new(
                            "extension-baseline",
                            "Claude Skill 基线规则指针无效",
                        ));
                    };
                    let (rendered, changes) =
                        apply_claude_skill_override_restore(&base, name, original_value.as_deref())
                            .map_err(adapter_error)?;
                    (rendered, !changes.is_empty())
                }
            };
            if restored {
                work.claim(&entry_pointer, writer)?;
                work.rendered = rendered;
                restored_slots.insert((target_path, entry_pointer));
            }
        }

        if restored_slots.is_empty() {
            return Ok((None, warnings));
        }
        let mut cleared = baseline.clone();
        cleared.entries.retain(|candidate| {
            !matches!(
                candidate,
                ManagedBaseline::DocumentEntry {
                    target_path: candidate_path,
                    entry_pointer: candidate_pointer,
                    ..
                } if restored_slots.contains(&(candidate_path.clone(), candidate_pointer.clone()))
            )
        });
        Ok((Some(cleared), warnings))
    }

    /// Re-enables a Claude private-project MCP. The private server and its
    /// disabledMcpServers member share the user document; both patches
    /// thread into the batch's single working text for it.
    #[allow(clippy::type_complexity)]
    pub(super) fn enable_private_mcp_steps(
        &self,
        definition: &ExtensionDefinition,
        binding: &ExtensionBinding,
        batch: &mut super::document::DocumentBatch,
        writer: usize,
    ) -> Result<
        (
            Vec<asb_core::extensions::mcp::EntryChange>,
            Option<ManagedBaselineFile>,
            Vec<String>,
        ),
        CommandError,
    > {
        let ExtensionPayload::Mcp(mcp) = &definition.payload else {
            return Err(CommandError::new("extension-invalid", "不是 MCP 定义"));
        };
        let target = self.mcp_document(binding)?;
        let McpScope::ClaudeProjectPrivate { project_path } = target.scope else {
            return Err(CommandError::new(
                "extension-invalid",
                "该 MCP 绑定不是 Claude 项目私有目标",
            ));
        };
        let document = target.path;
        if !self.document_baseline_is_current(binding, &document)? {
            return Err(CommandError::new(
                "extension-baseline",
                "MCP 绑定缺少可验证的部署基线，不能安全启用",
            ));
        }
        let key = binding
            .native_key
            .as_deref()
            .ok_or_else(|| CommandError::new("extension-invalid", "MCP 绑定缺少服务键"))?;
        let work = self.document_work(
            batch,
            &document,
            AppKind::Claude,
            DocumentSyntax::Json,
        )?;
        let base = work.rendered.clone();
        let render =
            render_claude(mcp, self.secrets, ClaudeHost::current()).map_err(projection_error)?;
        let (after_server, mut changes) = apply_claude_project_private_server_patches(
            &base,
            &project_path,
            &[(key.to_string(), Some(render))],
        )
        .map_err(adapter_error)?;
        let remove_disable_member =
            self.owns_removable_private_disable_member(binding, &document, &project_path, key)?;
        let (rendered, member_changes) = if remove_disable_member {
            apply_claude_project_disabled_members(
                &after_server,
                &project_path,
                &[],
                &[key.to_string()],
            )
            .map_err(adapter_error)?
        } else {
            (after_server, Vec::new())
        };
        if !changes.is_empty() {
            work.claim(&format!("projects.{project_path}.mcpServers.{key}"), writer)?;
        }
        if !member_changes.is_empty() {
            work.claim(
                &format!("projects.{project_path}.disabledMcpServers#{key}"),
                writer,
            )?;
        }
        if !changes.is_empty() || !member_changes.is_empty() {
            work.rendered = rendered;
        }
        let document_hash = sha_hex(work.rendered.as_bytes());
        let mut baseline = self.merge_baseline_entry(
            &binding.id,
            ManagedBaseline::DocumentEntry {
                target_path: document.to_string_lossy().to_string(),
                entry_pointer: format!("projects.{project_path}.mcpServers.{key}"),
                original_value: changes.first().and_then(|change| change.before.clone()),
                last_written_value: changes.first().and_then(|change| change.after.clone()),
                last_document_hash: document_hash.clone(),
                target_existed_before: work.expected_existed,
                backup_reference: None,
            },
        )?;
        if remove_disable_member {
            baseline = merge_baseline_file_entry(
                baseline,
                ManagedBaseline::SetMember {
                    target_path: document.to_string_lossy().to_string(),
                    collection_pointer: format!("projects.{project_path}.disabledMcpServers"),
                    member: key.to_string(),
                    original_present: false,
                    last_present: false,
                    last_document_hash: document_hash,
                    target_existed_before: work.expected_existed,
                    backup_reference: None,
                },
            );
        }
        let mut warnings = Vec::new();
        if !remove_disable_member
            && claude_project_disabled_member_present(&base, &project_path, key)
                .map_err(adapter_error)?
        {
            warnings.push("该项目已有非本应用写入的 MCP 停用规则；将保留该原生限制".to_string());
        }
        changes.extend(member_changes);
        Ok((changes, Some(baseline), warnings))
    }
}

mod disable;
