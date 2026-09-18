//! Removal planning: reversing one binding's client footprint. An
//! adopted entry or directory is restored from its takeover baseline
//! instead of deleted.

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    ExtensionBinding, ExtensionDefinition, ExtensionPayload, ExtensionTarget, ManagedBaseline,
    ManagedBaselineFile,
};
use asb_core::extensions::mcp::{
    apply_claude_project_disabled_members, apply_claude_project_private_server_patches,
    apply_claude_user_server_patches, apply_codex_entry_restore, apply_codex_server_patches,
};
use asb_core::extensions::plan::{PlannedFile, PlanStep};

use crate::commands::error::CommandError;
use crate::commands::extensions::support::*;

use super::deploy::write_entries_to;
use super::document::restore_claude_entry;
use super::{McpScope, Planner};

impl Planner<'_> {
    /// Document-level removal: disable rules plus the MCP entry itself.
    #[allow(clippy::type_complexity)]
    pub(super) fn remove_document_steps(
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
        match &definition.payload {
            ExtensionPayload::Mcp(mcp) => {
                if matches!(
                    &binding.target,
                    ExtensionTarget::ProjectPrivate {
                        client: AppKind::Claude,
                        ..
                    }
                ) {
                    self.remove_private_mcp_entry(binding, batch, writer)
                } else {
                    self.remove_mcp_entry(mcp, binding, batch, writer)
                }
            }
            ExtensionPayload::Skill(_) => Ok((Vec::new(), None, Vec::new())),
        }
    }

    /// Removes a Claude private-project server and, only when this binding
    /// added it, its disabledMcpServers member. Both native positions live
    /// in the same user document and thread into one working text.
    #[allow(clippy::type_complexity)]
    pub(super) fn remove_private_mcp_entry(
        &self,
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
                "MCP 绑定缺少可验证的部署基线，不能安全移除",
            ));
        }
        let key = binding
            .native_key
            .as_deref()
            .ok_or_else(|| CommandError::new("extension-invalid", "MCP 绑定缺少服务键"))?;
        let pointer = format!("projects.{project_path}.mcpServers.{key}");
        // A takeover baseline carries the native entry it adopted: removal
        // restores that text verbatim instead of deleting the entry.
        let takeover_original = self
            .store
            .get_baseline_file(&binding.id)
            .map_err(store_error)?
            .and_then(|file| {
                file.entries.into_iter().find_map(|entry| match entry {
                    ManagedBaseline::DocumentEntry {
                        entry_pointer,
                        original_value,
                        ..
                    } if entry_pointer == pointer => original_value,
                    _ => None,
                })
            });
        let work = self.document_work(
            batch,
            &document,
            AppKind::Claude,
            asb_core::extensions::contracts::DocumentSyntax::Json,
        )?;
        let base = work.rendered.clone();
        let (after_server, mut changes) = if let Some(original) = &takeover_original {
            restore_claude_entry(
                &base,
                &["projects", project_path.as_str(), "mcpServers"],
                key,
                original,
                &pointer,
            )?
        } else {
            apply_claude_project_private_server_patches(
                &base,
                &project_path,
                &[(key.to_string(), None)],
            )
            .map_err(adapter_error)?
        };
        let mut rendered = after_server;
        if self.owns_removable_private_disable_member(binding, &document, &project_path, key)? {
            let (after_members, member_changes) = apply_claude_project_disabled_members(
                &rendered,
                &project_path,
                &[],
                &[key.to_string()],
            )
            .map_err(adapter_error)?;
            changes.extend(member_changes);
            rendered = after_members;
        }
        if changes.is_empty() {
            return Ok((Vec::new(), None, Vec::new()));
        }
        work.claim(&pointer, writer)?;
        work.rendered = rendered;
        Ok((changes, None, Vec::new()))
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn remove_mcp_entry(
        &self,
        mcp: &asb_core::extensions::contracts::McpDefinition,
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
        let _ = mcp;
        let target = self.mcp_document(binding)?;
        let client = binding.target.client();
        let document = target.path.clone();
        if !self.document_baseline_is_current(binding, &document)? {
            return Err(CommandError::new(
                "extension-baseline",
                "MCP 绑定缺少可验证的部署基线，不能安全移除",
            ));
        }
        let key = binding
            .native_key
            .as_deref()
            .ok_or_else(|| CommandError::new("extension-invalid", "MCP 绑定缺少服务键"))?;
        let pointer = match &target.scope {
            McpScope::CodexServers => format!("mcp_servers.{key}"),
            McpScope::ClaudeUserServers => format!("mcpServers.{key}"),
            McpScope::ClaudeProjectPrivate { project_path } => {
                format!("projects.{project_path}.mcpServers.{key}")
            }
        };
        // A takeover baseline carries the native entry it adopted: removal
        // restores that text verbatim instead of deleting the entry.
        let takeover_original = self
            .store
            .get_baseline_file(&binding.id)
            .map_err(store_error)?
            .and_then(|file| {
                file.entries.into_iter().find_map(|entry| match entry {
                    ManagedBaseline::DocumentEntry {
                        entry_pointer,
                        original_value,
                        ..
                    } if entry_pointer == pointer => original_value,
                    _ => None,
                })
            });
        let work = self.document_work(batch, &document, client, target.syntax)?;
        let base = work.rendered.clone();
        let (rendered, changes) = match (&target.scope, &takeover_original) {
            (McpScope::CodexServers, Some(original)) => {
                let rendered =
                    apply_codex_entry_restore(&base, key, Some(original)).map_err(adapter_error)?;
                let before = apply_codex_server_patches(&base, &[(key.to_string(), None)])
                    .map_err(adapter_error)?
                    .1;
                (
                    rendered,
                    vec![asb_core::extensions::mcp::EntryChange {
                        pointer: pointer.clone(),
                        before: before.first().and_then(|change| change.before.clone()),
                        after: Some(original.clone()),
                    }],
                )
            }
            (McpScope::ClaudeUserServers, Some(original)) => {
                restore_claude_entry(&base, &["mcpServers"], key, original, &pointer)?
            }
            (McpScope::ClaudeProjectPrivate { project_path }, Some(original)) => {
                restore_claude_entry(
                    &base,
                    &["projects", project_path, "mcpServers"],
                    key,
                    original,
                    &pointer,
                )?
            }
            (_, None) => match &target.scope {
                McpScope::CodexServers => {
                    apply_codex_server_patches(&base, &[(key.to_string(), None)])
                        .map_err(adapter_error)?
                }
                McpScope::ClaudeUserServers => {
                    apply_claude_user_server_patches(&base, &[(key.to_string(), None)])
                        .map_err(adapter_error)?
                }
                McpScope::ClaudeProjectPrivate { project_path } => {
                    apply_claude_project_private_server_patches(
                        &base,
                        project_path,
                        &[(key.to_string(), None)],
                    )
                    .map_err(adapter_error)?
                }
            },
        };
        let baseline = self.merge_baseline_entry(
            &binding.id,
            ManagedBaseline::DocumentEntry {
                target_path: document.to_string_lossy().to_string(),
                entry_pointer: pointer.clone(),
                original_value: changes.first().and_then(|change| change.before.clone()),
                last_written_value: None,
                last_document_hash: sha_hex(rendered.as_bytes()),
                target_existed_before: work.expected_existed,
                backup_reference: None,
            },
        )?;
        if changes.is_empty() {
            return Ok((Vec::new(), Some(baseline), Vec::new()));
        }
        work.claim(&pointer, writer)?;
        work.rendered = rendered;
        Ok((changes, Some(baseline), Vec::new()))
    }

    /// The removal step for a managed skill directory. A directory this
    /// application deployed is deleted; an adopted directory (takeover
    /// baseline with an original digest) is restored to that library
    /// version instead of being deleted.
    pub(super) fn skill_remove_step(
        &self,
        definition: &ExtensionDefinition,
        binding: &ExtensionBinding,
    ) -> Result<Option<(PlanStep, bool)>, CommandError> {
        let ExtensionPayload::Skill(_) = &definition.payload else {
            return Ok(None);
        };
        let target_dir = self.skill_target_dir(binding)?;
        let Some(baseline) = self
            .store
            .get_baseline_file(&binding.id)
            .map_err(store_error)?
        else {
            return Ok(None);
        };
        let target = target_dir.to_string_lossy();
        let Some((last_digest, original_digest)) =
            baseline.entries.iter().find_map(|entry| match entry {
                ManagedBaseline::Directory {
                    target_dir,
                    last_digest,
                    original_digest,
                    ..
                } if target_dir == target.as_ref() => {
                    Some((last_digest.clone(), original_digest.clone()))
                }
                _ => None,
            })
        else {
            return Ok(None);
        };
        let Some(entries) = self.skill_target_entries(&target_dir)? else {
            return Ok(None);
        };
        let current_digest = asb_core::extensions::skill::content_digest(&entries);
        if current_digest != last_digest {
            return Err(CommandError::new(
                "extension-external-change",
                format!(
                    "{} 与本应用记录的 Skill 内容不一致，不能安全移除",
                    target_dir.display()
                ),
            ));
        }
        if let Some(original_digest) = original_digest {
            // Adopted directory: redeploy the takeover-time version.
            let original_entries = self
                .store
                .load_skill_version(&definition.id, &original_digest)
                .map_err(store_error)?;
            if asb_core::extensions::skill::content_digest(&original_entries) != original_digest {
                return Err(CommandError::new(
                    "extension-store",
                    "库中的接管原始版本与记录摘要不一致，不能恢复",
                ));
            }
            let staging_dir = std::env::temp_dir()
                .join("asb-extensions-staging")
                .join(&original_digest);
            write_entries_to(&original_entries, &staging_dir)?;
            let files: Vec<PlannedFile> = original_entries
                .iter()
                .map(|entry| PlannedFile {
                    relative_path: entry.relative_path.clone(),
                    digest: if entry.kind == asb_core::extensions::validate::ContentEntryKind::Dir {
                        String::new()
                    } else {
                        sha_hex(&entry.bytes)
                    },
                    mode: entry.mode,
                    size: entry.bytes.len() as u64,
                })
                .collect();
            return Ok(Some((
                PlanStep::DirectoryDeploy {
                    client: binding.target.client(),
                    target_dir: target_dir.to_string_lossy().to_string(),
                    staging_dir: staging_dir.to_string_lossy().to_string(),
                    files,
                    original_digest: Some(current_digest),
                    backup_dir: String::new(),
                },
                true,
            )));
        }
        let files: Vec<PlannedFile> = entries
            .iter()
            .map(|entry| PlannedFile {
                relative_path: entry.relative_path.clone(),
                digest: if entry.kind == asb_core::extensions::validate::ContentEntryKind::Dir {
                    String::new()
                } else {
                    sha_hex(&entry.bytes)
                },
                mode: entry.mode,
                size: entry.bytes.len() as u64,
            })
            .collect();
        Ok(Some((
            PlanStep::DirectoryRemove {
                client: binding.target.client(),
                target_dir: target_dir.to_string_lossy().to_string(),
                expected_files: files,
                expected_digest: current_digest,
                backup_dir: String::new(),
            },
            false,
        )))
    }
}
