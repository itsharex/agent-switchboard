//! The unified extension diagnostic contract: every warning the workspace
//! surfaces — scan-level discovery problems, per-entry discovery problems,
//! and managed-binding consistency problems — is one typed diagnostic with
//! a stable identity inside its scan, a machine-readable problem code, a
//! typed object reference, and an explicit remediation class.
//!
//! Diagnostics are facts of one scan. Real paths, content digests, baselines,
//! and repair parameters stay in the backend; consumers address a diagnostic
//! only through its id and the object reference it names.

use serde::{Deserialize, Serialize};

use crate::contracts::AppKind;
use crate::extensions::contracts::ExtensionKind;

/// What is wrong, as a stable machine code. Message text is display-only and
/// must never be matched on; the code is the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticCode {
    /// The skill directory has no `SKILL.md` at all.
    SkillManifestMissing,
    /// `SKILL.md` exists but carries no frontmatter block.
    SkillFrontmatterMissing,
    /// `SKILL.md` exists but its frontmatter cannot be parsed.
    SkillFrontmatterInvalid,
    /// A skill directory could not be read as content (I/O failure).
    SkillDirUnreadable,
    /// A skill directory contains a link or reparse point; never followed.
    SkillEntryLink,
    /// A skill directory contains an unsupported entry type.
    SkillEntryUnsupported,
    /// The client's skill root could not be listed.
    SkillRootUnreadable,
    /// One entry of the skill root could not be stat-ed.
    SkillRootEntryUnreadable,
    /// The MCP document exists but cannot be read.
    McpDocumentUnreadable,
    /// The MCP document exists but does not parse in the client's syntax.
    McpDocumentUnparsable,
    /// The MCP collection key exists but its type is wrong (not a table or
    /// object). The document must not be treated as having zero servers.
    McpCollectionInvalid,
    /// One MCP entry is not a table/object.
    McpEntryNotAnObject,
    /// One MCP entry carries neither a url nor a command.
    McpTransportMissing,
    /// One MCP entry carries conflicting transport fields (url and command).
    McpTransportConflicting,
    /// One MCP entry names an unknown transport type.
    McpTransportUnknown,
    /// The entry has fields this contract does not model; kept verbatim.
    McpUnknownFields,
    /// The entry carries a field the client ignores in this scope.
    McpIgnoredField,
    /// A managed target that was deployed is gone (directory or document).
    ManagedTargetMissing,
    /// A managed document entry is absent while the document itself is
    /// readable and otherwise at its baseline.
    ManagedEntryMissing,
    /// The managed target changed outside this application.
    ManagedTargetExternalChange,
    /// The managed target exists but cannot be read.
    ManagedTargetUnreadable,
}

/// What the workspace offers for one diagnostic, with the reason that
/// decides the class. Auto means the repair planner may re-verify and fix
/// it; Manual means a person decides; Info never counts as a fault.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DiagnosticRemediation {
    Auto { reason: String },
    Manual { reason: String },
    Info,
}

/// The typed object a diagnostic is about. Real paths never cross this
/// boundary: entries and bindings are named by id, scan locations by a
/// renderer-safe scope label.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum DiagnosticSubject {
    /// One row of the discovery table (observed skill or MCP entry).
    DiscoveryEntry { observation_id: String },
    /// One binding the library manages; repair resolves through it.
    ManagedBinding { binding_id: String },
    /// A scan location that produced no entry row (unreadable root,
    /// unparsable document). The label names the scope, never the path;
    /// `resource_kind` keeps the problem under its workspace tab.
    ScanLocation {
        label: String,
        resource_kind: ExtensionKind,
    },
}

/// One diagnostic of a discovery scan: an identity, a typed problem, an
/// object, and what can be done about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionDiagnostic {
    /// Unique inside the scan that produced it; the only handle consumers
    /// need in order to act on it.
    pub id: String,
    pub code: DiagnosticCode,
    pub client: AppKind,
    pub subject: DiagnosticSubject,
    pub message: String,
    pub remediation: DiagnosticRemediation,
}

impl ExtensionDiagnostic {
    /// Whether the repair batch may include this diagnostic.
    pub fn is_auto_repairable(&self) -> bool {
        matches!(self.remediation, DiagnosticRemediation::Auto { .. })
    }
}
