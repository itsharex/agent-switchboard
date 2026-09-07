use serde::{Deserialize, Serialize};

use crate::contracts::AppKind;

/// Version of the extension library file contracts. Strict parsing means a
/// mismatched version file is rejected, never migrated silently; moving to a
/// new version happens only through the one-shot offline migrator.
pub const EXTENSIONS_SCHEMA_VERSION: u8 = 3;

/// The concrete format of one managed client document. It belongs to the
/// persisted extension contract because a historical restore must write the
/// exact document that the original operation changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DocumentSyntax {
    Toml,
    Json,
}

/// Library-root manifest persisted as `extensions/manifest.json`. The
/// generation increases with every committed mutation of the library so
/// readers can detect concurrent writers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtensionManifest {
    pub schema_version: u8,
    pub generation: u64,
}

/// Which resource family an extension belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExtensionKind {
    Skill,
    Mcp,
}

impl ExtensionKind {
    pub fn label(self) -> &'static str {
        match self {
            ExtensionKind::Skill => "Skill",
            ExtensionKind::Mcp => "MCP",
        }
    }
}

/// Where a managed extension takes effect. Codex has no private project
/// document, so [`ExtensionTarget::ProjectPrivate`] is Claude-only; invalid
/// combinations are rejected by validation, not coerced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "scope",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ExtensionTarget {
    /// The client's user-level configuration.
    App { client: AppKind },
    /// Configuration shared with everyone working on one project.
    ProjectShared { client: AppKind, project_id: String },
    /// Configuration private to the current user inside one project.
    ProjectPrivate { client: AppKind, project_id: String },
}

impl ExtensionTarget {
    pub fn client(&self) -> AppKind {
        match self {
            ExtensionTarget::App { client } => *client,
            ExtensionTarget::ProjectShared { client, .. }
            | ExtensionTarget::ProjectPrivate { client, .. } => *client,
        }
    }

    pub fn project_id(&self) -> Option<&str> {
        match self {
            ExtensionTarget::App { .. } => None,
            ExtensionTarget::ProjectShared { project_id, .. }
            | ExtensionTarget::ProjectPrivate { project_id, .. } => Some(project_id),
        }
    }
}

/// The enabled/disabled intent stored on a binding. It is not the client's
/// runtime state; file consistency and native conditions are reported
/// separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DesiredState {
    Enabled,
    Disabled,
}

/// A value position that can carry sensitive material. Plain literals are
/// only legitimate for non-sensitive parameters; secret-shaped input must
/// arrive through the dedicated secret interface as a [`SecretValue::SecretRef`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum SecretValue {
    /// Reference to a host environment variable by exact name.
    EnvRef { name: String },
    /// Opaque handle into the system credential store.
    SecretRef { reference: String },
    /// Explicit literal value for non-sensitive parameters.
    Plain { value: String },
}
