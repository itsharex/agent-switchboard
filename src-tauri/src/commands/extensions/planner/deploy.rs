//! Deploy planning: rendering a definition's content into document
//! patches or a staged skill directory, with the target path helpers
//! and the in-memory baseline merge.

use std::fs;
use std::path::PathBuf;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    DesiredState, ExtensionBinding, ExtensionDefinition, ExtensionPayload, ExtensionTarget,
    ManagedBaseline, ManagedBaselineFile, ManagedFileEntry,
};
use asb_core::extensions::mcp::{
    apply_claude_project_private_server_patches, apply_claude_user_server_patches,
    apply_codex_server_patches, render_claude, render_codex, ClaudeHost,
};
use asb_core::extensions::plan::{PlanStep, PlannedFile};
use asb_core::extensions::skill::ContentEntry;

use crate::commands::error::CommandError;
use crate::commands::extensions::support::*;

use super::{McpScope, Planner};

impl Planner<'_> {
    /// Steps that put one binding's desired state onto its target. The
    /// document patch threads into the batch's working text; the single
    /// document write is materialized once the batch is complete.
    #[allow(clippy::type_complexity)]
    pub(super) fn deploy_steps(
        &self,
        definition: &ExtensionDefinition,
        binding: &ExtensionBinding,
        batch: &mut super::document::DocumentBatch,
        writer: usize,
    ) -> Result<
        (
            Vec<PlanStep>,
            Vec<asb_core::extensions::mcp::EntryChange>,
            Option<ManagedBaselineFile>,
            Vec<String>,
            bool,
        ),
        CommandError,
    > {
        match &definition.payload {
            ExtensionPayload::Mcp(mcp) => {
                let target = self.mcp_document(binding)?;
                let document = target.path.clone();
                let syntax = target.syntax;
                let client = binding.target.client();
                let has_document_baseline =
                    self.document_baseline_is_current(binding, &document)?;
                let key = binding
                    .native_key
                    .as_deref()
                    .ok_or_else(|| CommandError::new("extension-invalid", "MCP 绑定缺少服务键"))?;
                let enabled = binding.desired == DesiredState::Enabled;
                let work = self.document_work(batch, &document, client, syntax)?;
                let base = work.rendered.clone();
                let (rendered, changes, entry_pointer) = match &target.scope {
                    McpScope::CodexServers => {
                        let render =
                            render_codex(mcp, enabled, self.secrets).map_err(projection_error)?;
                        let (rendered, changes) =
                            apply_codex_server_patches(&base, &[(key.to_string(), Some(render))])
                                .map_err(adapter_error)?;
                        (rendered, changes, format!("mcp_servers.{key}"))
                    }
                    McpScope::ClaudeUserServers => {
                        let render = render_claude(mcp, self.secrets, ClaudeHost::current())
                            .map_err(projection_error)?;
                        let (rendered, changes) = apply_claude_user_server_patches(
                            &base,
                            &[(key.to_string(), if enabled { Some(render) } else { None })],
                        )
                        .map_err(adapter_error)?;
                        (rendered, changes, format!("mcpServers.{key}"))
                    }
                    McpScope::ClaudeProjectPrivate { project_path } => {
                        let render = render_claude(mcp, self.secrets, ClaudeHost::current())
                            .map_err(projection_error)?;
                        let (rendered, changes) = apply_claude_project_private_server_patches(
                            &base,
                            project_path,
                            &[(key.to_string(), if enabled { Some(render) } else { None })],
                        )
                        .map_err(adapter_error)?;
                        (
                            rendered,
                            changes,
                            format!("projects.{project_path}.mcpServers.{key}"),
                        )
                    }
                };
                // Deploying onto a native entry this application does not
                // own adopts it: the baseline records the original value as
                // the verbatim restore point, and the plan view forces the
                // explicit confirmation dialog. Managed collisions are
                // rejected earlier by `assert_native_key_free`.
                let adopts_native_entry = !has_document_baseline
                    && changes.first().and_then(|change| change.before.as_ref()).is_some();
                let mut warnings = Vec::new();
                let document_hash = sha_hex(rendered.as_bytes());
                if !changes.is_empty() {
                    work.claim(&entry_pointer, writer)?;
                    work.rendered = rendered;
                    if adopts_native_entry {
                        warnings.push(
                            "将替换同名原生条目；原内容已记录为恢复点，移除时恢复".to_string(),
                        );
                    }
                }
                if !enabled && client == AppKind::Claude {
                    warnings.push("将从 Claude 配置移除该服务条目，库中保留定义".to_string());
                }
                let baseline = self.merge_baseline_entry(
                    &binding.id,
                    ManagedBaseline::DocumentEntry {
                        target_path: document.to_string_lossy().to_string(),
                        entry_pointer,
                        original_value: changes.first().and_then(|change| change.before.clone()),
                        last_written_value: changes.first().and_then(|change| change.after.clone()),
                        // Materialization rewrites this to the batch's final
                        // document hash; unchanged documents keep the disk
                        // hash of the threaded text.
                        last_document_hash: document_hash,
                        target_existed_before: work.expected_existed,
                        backup_reference: None,
                    },
                )?;
                Ok((Vec::new(), changes, Some(baseline), warnings, adopts_native_entry))
            }
            ExtensionPayload::Skill(_) => {
                let (steps, changes, baseline, warnings) =
                    self.skill_deploy_steps(definition, binding)?;
                Ok((steps, changes, baseline, warnings, false))
            }
        }
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn skill_deploy_steps(
        &self,
        definition: &ExtensionDefinition,
        binding: &ExtensionBinding,
    ) -> Result<
        (
            Vec<PlanStep>,
            Vec<asb_core::extensions::mcp::EntryChange>,
            Option<ManagedBaselineFile>,
            Vec<String>,
        ),
        CommandError,
    > {
        let ExtensionPayload::Skill(skill) = &definition.payload else {
            return Err(CommandError::new("extension-invalid", "不是 Skill 定义"));
        };
        if binding.desired != DesiredState::Enabled {
            return Ok((
                Vec::new(),
                Vec::new(),
                None,
                vec!["已按停用意图入库，未部署".to_string()],
            ));
        }
        // A pinned binding deploys exactly its locked version, never the
        // definition's newer content; the version store owns the bytes.
        let deploy_digest = binding
            .locked_digest
            .clone()
            .unwrap_or_else(|| skill.content_digest.clone());
        let entries = self
            .store
            .load_skill_version(&definition.id, &deploy_digest)
            .map_err(store_error)?;
        let target_dir = self.skill_target_dir(binding)?;
        let current_entries = self.skill_target_entries(&target_dir)?;
        self.ensure_skill_target_ownership(binding, &target_dir, current_entries.as_deref())?;
        let original_digest = current_entries
            .as_ref()
            .map(|existing| asb_core::extensions::skill::content_digest(existing));
        let mut planned_files: Vec<PlannedFile> = Vec::new();
        for entry in &entries {
            planned_files.push(PlannedFile {
                relative_path: entry.relative_path.clone(),
                digest: if entry.kind == asb_core::extensions::validate::ContentEntryKind::Dir {
                    String::new()
                } else {
                    sha_hex(&entry.bytes)
                },
                mode: entry.mode,
                size: entry.bytes.len() as u64,
            });
        }
        // Staging copy for the executor; prepared before any lock is taken.
        let staging_dir = std::env::temp_dir()
            .join("asb-extensions-staging")
            .join(&deploy_digest);
        write_entries_to(&entries, &staging_dir)?;
        let baseline = self.merge_baseline_entry(
            &binding.id,
            ManagedBaseline::Directory {
                target_dir: target_dir.to_string_lossy().to_string(),
                original_existed: original_digest.is_some(),
                original_files: None,
                original_digest: None,
                last_files: planned_files
                    .iter()
                    .map(|file| ManagedFileEntry {
                        relative_path: file.relative_path.clone(),
                        digest: file.digest.clone(),
                        mode: file.mode,
                        size: file.size,
                    })
                    .collect(),
                last_digest: deploy_digest.clone(),
                backup_reference: None,
            },
        )?;
        let steps = vec![PlanStep::DirectoryDeploy {
            client: binding.target.client(),
            target_dir: target_dir.to_string_lossy().to_string(),
            staging_dir: staging_dir.to_string_lossy().to_string(),
            files: planned_files,
            original_digest,
            backup_dir: String::new(),
        }];
        Ok((steps, Vec::new(), Some(baseline), Vec::new()))
    }

    pub(super) fn project_path(&self, binding: &ExtensionBinding) -> Result<String, CommandError> {
        let project_id = binding
            .target
            .project_id()
            .ok_or_else(|| CommandError::new("extension-invalid", "项目绑定缺少项目 id"))?;
        let projects = self.store.list_projects().map_err(store_error)?;
        projects
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| project.root.clone())
            .ok_or_else(|| {
                CommandError::new("extension-invalid", "项目位置已失效；请重新注册并选择项目")
            })
    }

    pub(super) fn skill_target_dir(
        &self,
        binding: &ExtensionBinding,
    ) -> Result<PathBuf, CommandError> {
        let name = binding
            .deploy_name
            .as_deref()
            .ok_or_else(|| CommandError::new("extension-invalid", "Skill 绑定缺少部署名"))?;
        match &binding.target {
            ExtensionTarget::App { client } => Ok(match client {
                AppKind::Codex => {
                    crate::extensions::paths::codex_user_skills_root(&self.paths.home)
                }
                AppKind::Claude => crate::extensions::paths::claude_user_skills_root(
                    &self.paths.home,
                    self.paths.claude_dir.as_deref(),
                ),
            }
            .join(name)),
            ExtensionTarget::ProjectShared {
                project_id: _,
                client,
            }
            | ExtensionTarget::ProjectPrivate {
                project_id: _,
                client,
            } => {
                let root = std::path::PathBuf::from(self.project_path(binding)?);
                Ok(match client {
                    AppKind::Codex => crate::extensions::paths::codex_project_skills_root(&root),
                    AppKind::Claude => crate::extensions::paths::claude_project_skills_root(&root),
                }
                .join(name))
            }
        }
    }
}

/// Adds or refreshes exactly one owned native position while retaining the
/// original value observed when this application first took ownership.
/// Callers that update multiple positions in one document use this in-memory
/// form so their planned baseline remains a single coherent post-state.
pub(super) fn merge_baseline_file_entry(
    mut file: ManagedBaselineFile,
    entry: ManagedBaseline,
) -> ManagedBaselineFile {
    let existing = file
        .entries
        .iter()
        .find(|existing| existing.same_slot(&entry));
    let mut merged = entry;
    match (existing, &mut merged) {
        (
            Some(ManagedBaseline::DocumentEntry {
                original_value,
                target_existed_before,
                ..
            }),
            ManagedBaseline::DocumentEntry {
                original_value: next_original_value,
                target_existed_before: next_target_existed_before,
                ..
            },
        ) => {
            *next_original_value = original_value.clone();
            *next_target_existed_before = *target_existed_before;
        }
        (
            Some(ManagedBaseline::SetMember {
                original_present,
                target_existed_before,
                ..
            }),
            ManagedBaseline::SetMember {
                original_present: next_original_present,
                target_existed_before: next_target_existed_before,
                ..
            },
        ) => {
            *next_original_present = *original_present;
            *next_target_existed_before = *target_existed_before;
        }
        (
            Some(ManagedBaseline::Directory {
                original_existed,
                original_files,
                original_digest,
                ..
            }),
            ManagedBaseline::Directory {
                original_existed: next_original_existed,
                original_files: next_original_files,
                original_digest: next_original_digest,
                ..
            },
        ) => {
            *next_original_existed = *original_existed;
            *next_original_files = original_files.clone();
            *next_original_digest = original_digest.clone();
        }
        _ => {}
    }
    file.entries.retain(|existing| !existing.same_slot(&merged));
    file.entries.push(merged);
    file
}

pub(super) fn required_id<'a>(
    value: &'a Option<String>,
    message: &str,
) -> Result<&'a str, CommandError> {
    value
        .as_deref()
        .ok_or_else(|| CommandError::new("extension-invalid", message))
}

pub(super) fn write_entries_to(
    entries: &[ContentEntry],
    root: &std::path::Path,
) -> Result<(), CommandError> {
    for entry in entries {
        let path = root.join(&entry.relative_path);
        match entry.kind {
            asb_core::extensions::validate::ContentEntryKind::Dir => {
                fs::create_dir_all(&path)
                    .map_err(|error| CommandError::new("extension-staging", error.to_string()))?;
            }
            asb_core::extensions::validate::ContentEntryKind::File => {
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent).map_err(|error| {
                        CommandError::new("extension-staging", error.to_string())
                    })?;
                }
                fs::write(&path, &entry.bytes)
                    .map_err(|error| CommandError::new("extension-staging", error.to_string()))?;
            }
            asb_core::extensions::validate::ContentEntryKind::Link => {
                return Err(CommandError::new(
                    "extension-staging",
                    "Skill 暂存内容不能包含链接或重解析点",
                ));
            }
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(entry.mode & 0o7777))
                .map_err(|error| CommandError::new("extension-staging", error.to_string()))?;
        }
    }
    Ok(())
}
