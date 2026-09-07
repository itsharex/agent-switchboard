use serde::{Deserialize, Serialize};

use crate::extensions::contracts::EXTENSIONS_SCHEMA_VERSION;

/// One managed file inside a directory baseline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedFileEntry {
    /// `/`-separated path relative to the managed directory root.
    pub relative_path: String,
    pub digest: String,
    /// Unix mode bits captured with the content version.
    pub mode: u32,
    pub size: u64,
}

/// Restricted local metadata describing everything an application owns at a
/// target, so enable/restore can undo exactly its own writes. Persisted as
/// `extensions/baselines/<binding-id>.json` and never sent to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ManagedBaseline {
    /// One document entry (native MCP server, skill override, enable rule)
    /// that the application owns. `original_value` is the canonical text of
    /// the entry before the first application, absent when the entry did not
    /// exist; `last_written_value` is what the application last wrote,
    /// absent when the last operation removed it.
    DocumentEntry {
        target_path: String,
        /// Owner-defined pointer to the entry inside the document.
        entry_pointer: String,
        original_value: Option<String>,
        last_written_value: Option<String>,
        /// Whole-file digest observed right after the last write.
        last_document_hash: String,
        target_existed_before: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        backup_reference: Option<String>,
    },
    /// One member of a collection such as Claude's `disabledMcpServers`.
    SetMember {
        target_path: String,
        collection_pointer: String,
        member: String,
        original_present: bool,
        last_present: bool,
        last_document_hash: String,
        target_existed_before: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        backup_reference: Option<String>,
    },
    /// A managed skill directory. The manifests allow detecting external
    /// additions, modifications, and removals file by file.
    Directory {
        target_dir: String,
        original_existed: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        original_files: Option<Vec<ManagedFileEntry>>,
        last_files: Vec<ManagedFileEntry>,
        last_digest: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        /// Whole-directory digest captured when ownership was taken over
        /// from an existing native installation; removing the binding
        /// restores that library version instead of deleting the directory.
        /// Absent for directories this application created.
        original_digest: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        backup_reference: Option<String>,
    },
}

/// The complete persisted baseline of one binding: everything the
/// application owns at its target — the deployed directory (when any) plus
/// every document entry or collection member this binding owns (skill
/// visibility rules, disable members). One binding has exactly one baseline
/// file; partial operations extend it instead of replacing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManagedBaselineFile {
    pub schema_version: u8,
    pub binding_id: String,
    pub entries: Vec<ManagedBaseline>,
}

impl ManagedBaselineFile {
    pub fn new(binding_id: &str, entries: Vec<ManagedBaseline>) -> Self {
        Self {
            schema_version: EXTENSIONS_SCHEMA_VERSION,
            binding_id: binding_id.to_string(),
            entries,
        }
    }

    pub fn entry_of_kind(&self, kind: BaselineKind) -> Option<&ManagedBaseline> {
        self.entries.iter().find(|entry| entry.is_kind(kind))
    }
}

/// The discriminator of a [`ManagedBaseline`] variant, used for lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaselineKind {
    DocumentEntry,
    SetMember,
    Directory,
}

impl ManagedBaseline {
    pub fn is_kind(&self, kind: BaselineKind) -> bool {
        matches!(
            (self, kind),
            (
                ManagedBaseline::DocumentEntry { .. },
                BaselineKind::DocumentEntry
            ) | (ManagedBaseline::SetMember { .. }, BaselineKind::SetMember)
                | (ManagedBaseline::Directory { .. }, BaselineKind::Directory)
        )
    }

    /// Whether two entries own the same exact native position. The document
    /// path is part of the identity: an override in a project-local settings
    /// file is never interchangeable with an identically named override in
    /// the shared settings file.
    pub fn same_slot(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::DocumentEntry {
                    target_path: path_a,
                    entry_pointer: pointer_a,
                    ..
                },
                Self::DocumentEntry {
                    target_path: path_b,
                    entry_pointer: pointer_b,
                    ..
                },
            ) => path_a == path_b && pointer_a == pointer_b,
            (
                Self::SetMember {
                    target_path: path_a,
                    collection_pointer: pointer_a,
                    member: member_a,
                    ..
                },
                Self::SetMember {
                    target_path: path_b,
                    collection_pointer: pointer_b,
                    member: member_b,
                    ..
                },
            ) => path_a == path_b && pointer_a == pointer_b && member_a == member_b,
            (
                Self::Directory {
                    target_dir: path_a, ..
                },
                Self::Directory {
                    target_dir: path_b, ..
                },
            ) => path_a == path_b,
            _ => false,
        }
    }
}

#[cfg(test)]
mod managed_baseline_tests {
    use super::ManagedBaseline;

    fn document(path: &str) -> ManagedBaseline {
        ManagedBaseline::DocumentEntry {
            target_path: path.to_string(),
            entry_pointer: "skillOverrides.example".to_string(),
            original_value: None,
            last_written_value: Some(r#""off""#.to_string()),
            last_document_hash: "digest".to_string(),
            target_existed_before: true,
            backup_reference: None,
        }
    }

    #[test]
    fn same_slot_keeps_shared_and_local_documents_independent() {
        assert!(!document("project/.claude/settings.json")
            .same_slot(&document("project/.claude/settings.local.json")));
        assert!(document("project/.claude/settings.json")
            .same_slot(&document("project/.claude/settings.json")));
    }
}
