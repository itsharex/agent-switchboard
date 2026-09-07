use serde::{Deserialize, Serialize};

use crate::contracts::AppKind;
use crate::extensions::contracts::{
    DocumentSyntax, ExtensionBinding, ExtensionKind, ExtensionTarget, ManagedBaselineFile,
};

/// The persisted, restricted snapshot of one finished operation: the
/// record, the library state before it, and the library state after it.
/// History restore replays exactly this; nothing is inferred from current
/// baselines. Never serialized to the frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationSnapshot {
    pub schema_version: u8,
    pub record: ExtensionOperationRecord,
    /// Every completed client-file step, in execution order. This is local
    /// recovery data, never exposed through IPC; restore replays its exact
    /// inverse rather than deriving paths from current definitions.
    pub completed_steps: Vec<AppliedExtensionStep>,
    /// Baseline files as they were before the operation, keyed by binding.
    pub pre_baselines: Vec<(String, ManagedBaselineFile)>,
    /// Bindings that existed before the operation (including ones the
    /// operation removed).
    pub pre_bindings: Vec<ExtensionBinding>,
    /// Bindings in their post-operation state.
    pub post_bindings: Vec<ExtensionBinding>,
    /// Baseline files in their post-operation state.
    pub post_baselines: Vec<(String, ManagedBaselineFile)>,
}

/// One completed filesystem step tied to its original extension target and
/// resource. Both identities are stored with the step so historical
/// restoration remains valid after a definition or project registration
/// changes, and so a batch that touches the same target through several
/// resources (a skill directory plus an MCP document entry) can reverse
/// each step under the resource that owns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AppliedExtensionStep {
    pub resource_id: String,
    pub target: ExtensionTarget,
    pub step: AppliedStep,
}

/// Backend-local facts needed to verify and reverse one completed client
/// filesystem operation. Backup paths never cross the frontend boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AppliedStep {
    DocumentWritten {
        path: String,
        syntax: DocumentSyntax,
        backup_reference: Option<String>,
        written_hash: String,
        original_hash: Option<String>,
    },
    DocumentRemoved {
        path: String,
        syntax: DocumentSyntax,
        backup_reference: String,
        original_hash: String,
    },
    DirectoryDeployed {
        target_dir: String,
        backup_reference: Option<String>,
        digest: String,
        original_digest: Option<String>,
    },
    DirectoryRemoved {
        target_dir: String,
        backup_reference: String,
        digest: String,
        original_existed: bool,
    },
}

impl AppliedStep {
    pub fn backup_reference(&self) -> Option<&str> {
        match self {
            Self::DocumentWritten {
                backup_reference, ..
            }
            | Self::DirectoryDeployed {
                backup_reference, ..
            } => backup_reference.as_deref(),
            Self::DocumentRemoved {
                backup_reference, ..
            }
            | Self::DirectoryRemoved {
                backup_reference, ..
            } => Some(backup_reference),
        }
    }
}

/// A registered local project, persisted as `extensions/projects/<id>.json`.
/// Projects live only on this machine and are never part of cloud backups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectRegistration {
    pub schema_version: u8,
    pub id: String,
    /// Absolute, canonically resolved project root.
    pub root: String,
    pub display_name: String,
    pub registered_at: String,
}

/// Where an observation came from, recorded read-only per scan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "origin",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ObservedOrigin {
    /// The client's current user-level skill root.
    UserRoot,
    /// A historical or built-in location; never migrated automatically.
    LegacyRoot { detail: String },
    /// A project-relative skill root.
    ProjectRoot { project_path: String },
    /// Inside a plugin or managed installation; read-only forever.
    Managed { detail: String },
}

/// File consistency between the last application and the current disk state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileState {
    /// The binding exists but nothing has been applied yet.
    NotDeployed,
    /// The disk content matches the last application.
    InSync,
    /// The definition or intent moved past the last application.
    PendingApply,
    /// The disk content no longer matches the application baseline.
    ExternalChange,
    /// A previously deployed target no longer exists.
    Missing,
    /// The target exists but cannot be read or parsed.
    Unreadable,
}

/// A native condition that limits or explains a target's effect, observed
/// from readable configuration. Unprovable facts are reported as unknown by
/// the caller instead of being invented here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum NativeCondition {
    /// The native definition carries `enabled = false` (Codex).
    DisabledNatively { detail: String },
    /// A same-name item shadows this one under client-specific precedence.
    Overridden { detail: String },
    /// The project has not granted the client's trust for this scope.
    PendingTrust { detail: String },
    /// A managed/system policy or plugin provides the same resource.
    PolicyLimited { detail: String },
    /// A capability required for full effect was never verified.
    UnverifiedCapability { detail: String },
}

/// One observed skill or MCP server from a discovery scan. Observations are
/// facts from one point in time; they never become a second persistent
/// desired-state record.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedExtension {
    pub kind: ExtensionKind,
    pub client: AppKind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub origin: ObservedOrigin,
    /// Absolute path of the skill directory or the owning document.
    pub path: String,
    /// Bindings that currently manage this observed item.
    pub managed_binding_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_digest: Option<String>,
    /// Transport label for observed MCP servers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<String>,
}

/// How a skill/MCP dependency resolves against the library and a target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DependencyState {
    /// Linked to a library MCP definition.
    Bound,
    /// Declared but not linked to any library definition yet.
    PendingConfiguration,
    /// Linked, but the definition cannot be projected to the target client.
    TargetUnsupported,
}

/// The per-client capability report derived from the verified-capability
/// table. Unknown client versions may still be discovered and previewed;
/// writes backed by unverified capabilities are refused at the command layer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientCapabilityReport {
    pub client: AppKind,
    pub entries: Vec<CapabilityEntry>,
}

/// One capability line: which resource rule is usable and how it was
/// verified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapabilityEntry {
    /// Stable machine code, e.g. `skill-user`.
    pub code: String,
    /// Display text naming the resource and its path rule.
    pub resource: String,
    pub supported: bool,
    #[serde(flatten)]
    pub verification: CapabilityVerification,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verification", rename_all = "camelCase")]
pub enum CapabilityVerification {
    /// Behavior was confirmed against the named client release.
    Verified {
        client_version: String,
        verified_on: String,
    },
    /// Not yet confirmed; the condition names the required isolation test.
    Open { condition: String },
}

/// The outcome of one explicit MCP connection check. A passing check never
/// claims that the native client can reach the server; it describes this
/// one bounded probe only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpCheckResult {
    pub definition_id: String,
    pub definition_revision: u64,
    pub target: ExtensionTarget,
    /// RFC 3339 UTC timestamp of the check.
    pub checked_at: String,
    /// The MCP protocol version the probe settled on, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol_version: Option<String>,
    pub outcome: McpCheckOutcome,
    pub duration_ms: u64,
    /// True when directory contents were cut off by output or page limits.
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum McpCheckOutcome {
    /// Initialize and directory reads succeeded.
    Passed {
        tools: u64,
        resources: u64,
        prompts: u64,
    },
    /// The server answered but at least one directory read failed.
    Partial { error: String },
    /// The probe failed; classification and sanitized message are included.
    Failed {
        classification: String,
        error: String,
    },
    /// The user cancelled the probe.
    Cancelled,
    /// The server requires an interactive login that only the native client
    /// can perform; no verdict about the host session is implied.
    NeedsNativeConfirmation,
}

/// One per-target result inside a finished extension operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum TargetOutcome {
    Applied,
    Failed {
        message: String,
    },
    /// The target was restored to its previous content after a failure.
    Restored {
        message: String,
    },
    /// A restore was attempted and failed; manual recovery is required.
    RestoreFailed {
        message: String,
        backup: String,
    },
    /// The target was not part of the executed plan (for example after the
    /// user removed it from a batch).
    Skipped {
        message: String,
    },
}

/// One resource's slice of a finished batch operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationResourceRecord {
    pub definition_id: String,
    pub definition_revision: u64,
    /// The operation applied to this resource; one batch may mix them.
    pub operation: PlanOperation,
    pub targets: Vec<ExtensionTargetResult>,
}

/// The redacted, displayable record of one finished batch operation,
/// persisted as `extensions/history/<operation-id>.json`. One record covers
/// every resource the batch touched; restore replays the same grouping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionOperationRecord {
    pub schema_version: u8,
    pub id: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finished_at: Option<String>,
    pub resources: Vec<OperationResourceRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback: Option<RollbackSummary>,
}

/// One target's recorded result. Backups may contain credentials; this
/// record only references their location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionTargetResult {
    pub target: ExtensionTarget,
    pub outcome: TargetOutcome,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// What the rollback pass achieved for one operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RollbackSummary {
    pub restored: Vec<String>,
    pub failed: Vec<String>,
}

/// The set of plan operations the workspace supports. Restore replans a
/// previous operation's inverse and goes through the same preview flow.
/// Repair restores the last validly deployed state of managed objects the
/// discovery scan found damaged; it never advances the applied revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PlanOperation {
    Install,
    Update,
    Enable,
    Disable,
    Remove,
    Restore,
    Repair,
}

impl PlanOperation {
    pub fn label(self) -> &'static str {
        match self {
            PlanOperation::Install => "安装",
            PlanOperation::Update => "更新",
            PlanOperation::Enable => "启用",
            PlanOperation::Disable => "停用",
            PlanOperation::Remove => "移除",
            PlanOperation::Restore => "恢复",
            PlanOperation::Repair => "修复",
        }
    }
}
