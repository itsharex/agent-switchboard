//! Capability and baseline preconditions every plan verifies before it
//! may touch a client document or skill directory.

use std::fs;

use asb_core::extensions::contracts::{
    CapabilityVerification, ExtensionBinding, ExtensionDefinition, ManagedBaseline,
    ManagedBaselineFile, PlanOperation,
};
use asb_core::extensions::skill::ContentEntry;
use asb_core::extensions::CapabilityEnvironment;
use asb_switch::io::PathKind;
use asb_switch::{FsIo, SwitchIo};

use crate::commands::error::CommandError;
use crate::commands::extensions::support::*;

use super::deploy::merge_baseline_file_entry;
use super::Planner;

impl Planner<'_> {
    pub(crate) fn require_write_capabilities(
        &self,
        definition: &ExtensionDefinition,
        binding: &ExtensionBinding,
        operation: PlanOperation,
    ) -> Result<(), CommandError> {
        let environment = CapabilityEnvironment {
            claude_config_dir_custom: self.paths.claude_dir.is_some(),
        };
        let report = asb_core::extensions::capability_report(binding.target.client(), &environment);
        let changes_visibility = matches!(
            operation,
            PlanOperation::Enable | PlanOperation::Disable | PlanOperation::Remove
        );
        for code in asb_core::extensions::required_capability_codes(
            definition.kind(),
            &binding.target,
            changes_visibility,
        ) {
            let entry = report
                .entries
                .iter()
                .find(|entry| entry.code == code)
                .ok_or_else(|| {
                    CommandError::new(
                        "extension-capability",
                        format!("当前客户端能力表缺少 {code}，无法安全写入"),
                    )
                })?;
            if !entry.supported {
                let reason = match &entry.verification {
                    CapabilityVerification::Open { condition } => condition.as_str(),
                    CapabilityVerification::Verified { .. } => "当前环境不支持该能力",
                };
                return Err(CommandError::new(
                    "extension-capability",
                    format!("{} 当前不可写入：{reason}", entry.resource),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn document_baseline_is_current(
        &self,
        binding: &ExtensionBinding,
        document: &std::path::Path,
    ) -> Result<bool, CommandError> {
        let Some(baseline) = self
            .store
            .get_baseline_file(&binding.id)
            .map_err(store_error)?
        else {
            return Ok(false);
        };
        let document_path = document.to_string_lossy();
        let Some(last_document_hash) = baseline.entries.iter().find_map(|entry| match entry {
            ManagedBaseline::DocumentEntry {
                target_path,
                last_document_hash,
                ..
            }
            | ManagedBaseline::SetMember {
                target_path,
                last_document_hash,
                ..
            } if target_path == document_path.as_ref() => Some(last_document_hash),
            ManagedBaseline::DocumentEntry { .. }
            | ManagedBaseline::SetMember { .. }
            | ManagedBaseline::Directory { .. } => None,
        }) else {
            return Ok(false);
        };
        let bytes = fs::read(document).map_err(|_| {
            CommandError::new(
                "extension-external-change",
                format!("无法读取 {}；请先解决外部变更", document.display()),
            )
        })?;
        let external_change = || {
            CommandError::new(
                "extension-external-change",
                format!(
                    "{} 已在本应用上次写入后发生外部变更；请先解决冲突",
                    document.display()
                ),
            )
        };
        let text = String::from_utf8(bytes).map_err(|_| external_change())?;
        // 文档级哈希只作快路径（E02 条目级所有权）：切换投影、片段合并等
        // 第一方写入会改文档其他部分，但保留非自有内容；只要本应用拥有的
        // 每个条目仍与最后写入一致，文档就仍然可安全管理。条目本身变了
        // 才拒绝，等待用户显式解决。
        if sha_hex(text.as_bytes()) == *last_document_hash {
            return Ok(true);
        }
        let projects = self.store.list_projects().map_err(store_error)?;
        for entry in baseline.entries.iter().filter(|entry| {
            matches!(
                entry,
                ManagedBaseline::DocumentEntry { target_path, .. }
                    | ManagedBaseline::SetMember { target_path, .. }
                        if target_path == document_path.as_ref()
            )
        }) {
            let entry_current = match entry {
                ManagedBaseline::DocumentEntry {
                    last_written_value, ..
                } => {
                    let current = asb_core::extensions::mcp::managed_entry_text(
                        binding,
                        &text,
                        &projects,
                    )
                    .map_err(adapter_error)?;
                    asb_core::extensions::mcp::managed_entry_unchanged(
                        binding.target.client(),
                        current,
                        last_written_value.as_deref(),
                    )
                }
                ManagedBaseline::SetMember {
                    member,
                    last_present,
                    ..
                } => {
                    let Some(project_id) = binding.target.project_id() else {
                        return Err(external_change());
                    };
                    let Some(project_root) = projects
                        .iter()
                        .find(|project| project.id == *project_id)
                        .map(|project| project.root.clone())
                    else {
                        return Err(external_change());
                    };
                    asb_core::extensions::mcp::claude_project_disabled_member_present(
                        &text,
                        &project_root,
                        member,
                    )
                    .map_err(adapter_error)?
                        == *last_present
                }
                ManagedBaseline::Directory { .. } => continue,
            };
            if !entry_current {
                return Err(external_change());
            }
        }
        Ok(true)
    }

    /// A binding may write a document for the first time, but it may never
    /// overwrite a document position it already owns after that position
    /// changed outside this application. First-party rewrites of the rest of
    /// the document (provider switches, fragment merges) do not block the
    /// binding: ownership is judged per owned entry, not per document.
    pub(super) fn require_owned_document_baseline_current(
        &self,
        binding: &ExtensionBinding,
        document: &std::path::Path,
    ) -> Result<(), CommandError> {
        let Some(baseline) = self
            .store
            .get_baseline_file(&binding.id)
            .map_err(store_error)?
        else {
            return Ok(());
        };
        let path = document.to_string_lossy();
        let owns_document = baseline.entries.iter().any(|entry| {
            matches!(
                entry,
                ManagedBaseline::DocumentEntry { target_path, .. }
                    | ManagedBaseline::SetMember { target_path, .. }
                    if target_path == path.as_ref()
            )
        });
        if owns_document && !self.document_baseline_is_current(binding, document)? {
            return Err(CommandError::new(
                "extension-baseline",
                format!("{} 缺少可验证的部署基线，不能安全修改", document.display()),
            ));
        }
        Ok(())
    }

    /// Only remove a Claude private-project disable member when this binding
    /// added it. A member already present before the first disable belongs to
    /// the native configuration and must keep limiting the service.
    pub(super) fn owns_removable_private_disable_member(
        &self,
        binding: &ExtensionBinding,
        document: &std::path::Path,
        project_path: &str,
        member: &str,
    ) -> Result<bool, CommandError> {
        let Some(baseline) = self
            .store
            .get_baseline_file(&binding.id)
            .map_err(store_error)?
        else {
            return Ok(false);
        };
        let target_path = document.to_string_lossy();
        let collection_pointer = format!("projects.{project_path}.disabledMcpServers");
        Ok(baseline.entries.iter().any(|entry| {
            matches!(
                entry,
                ManagedBaseline::SetMember {
                    target_path: entry_path,
                    collection_pointer: entry_pointer,
                    member: entry_member,
                    original_present: false,
                    last_present: true,
                    ..
                } if entry_path == target_path.as_ref()
                    && entry_pointer == &collection_pointer
                    && entry_member == member
            )
        }))
    }

    pub(super) fn skill_target_entries(
        &self,
        target_dir: &std::path::Path,
    ) -> Result<Option<Vec<ContentEntry>>, CommandError> {
        let io = FsIo;
        match io.path_kind(target_dir).map_err(|_| {
            CommandError::new(
                "extension-external-change",
                format!("无法读取 {} 的文件类型", target_dir.display()),
            )
        })? {
            PathKind::Absent => Ok(None),
            PathKind::Directory => asb_switch::extensions::walk_content_entries(&io, target_dir)
                .map(Some)
                .map_err(|error| CommandError::new("extension-external-change", error.to_string())),
            PathKind::File { .. } | PathKind::Other => Err(CommandError::new(
                "extension-external-change",
                format!("{} 不是可由扩展管理的普通目录", target_dir.display()),
            )),
        }
    }

    pub(super) fn ensure_skill_target_ownership(
        &self,
        binding: &ExtensionBinding,
        target_dir: &std::path::Path,
        current_entries: Option<&[ContentEntry]>,
    ) -> Result<(), CommandError> {
        let baseline = self
            .store
            .get_baseline_file(&binding.id)
            .map_err(store_error)?;
        let target = target_dir.to_string_lossy();
        let directory = baseline.as_ref().and_then(|file| {
            file.entries.iter().find_map(|entry| match entry {
                ManagedBaseline::Directory {
                    target_dir,
                    last_digest,
                    ..
                } if target_dir == target.as_ref() => Some(last_digest),
                _ => None,
            })
        });
        match (directory, current_entries) {
            (Some(last_digest), Some(entries))
                if asb_core::extensions::skill::content_digest(entries) == *last_digest =>
            {
                Ok(())
            }
            (Some(_), _) => Err(CommandError::new(
                "extension-external-change",
                format!(
                    "{} 与本应用记录的 Skill 内容不一致；请先解决外部变更",
                    target_dir.display()
                ),
            )),
            (None, None) => Ok(()),
            (None, Some(_)) => Err(CommandError::new(
                "extension-conflict",
                format!(
                    "{} 已有未受本应用管理的 Skill 内容；请先显式导入并接管",
                    target_dir.display()
                ),
            )),
        }
    }

    /// Merges one owned baseline entry into the binding's existing file:
    /// the same pointer is replaced (the first-application original value
    /// survives), every other entry — the deployed directory, other rules —
    /// is preserved. Disable and enable own their rule entries; the
    /// directory ownership never resets.
    pub(super) fn merge_baseline_entry(
        &self,
        binding_id: &str,
        entry: ManagedBaseline,
    ) -> Result<ManagedBaselineFile, CommandError> {
        let file = self
            .store
            .get_baseline_file(binding_id)
            .map_err(store_error)?
            .unwrap_or_else(|| ManagedBaselineFile::new(binding_id, Vec::new()));
        Ok(merge_baseline_file_entry(file, entry))
    }
}
