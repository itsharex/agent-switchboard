use serde::{Deserialize, Serialize};

use crate::contracts::AppKind;

/// One skill's parsed `SKILL.md` frontmatter. Only the portable specification
/// fields plus the known Claude host fields are extracted; every other
/// frontmatter key is reported as an unparsed diagnostic and the original
/// file bytes stay untouched in the library copy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillManifest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Claude host field; it has no portable meaning and is never projected
    /// to Codex.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,
    /// Frontmatter keys that were left uninterpreted (names only, no values).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unparsed_keys: Vec<String>,
}

/// Provenance of library content that came from an external source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceRef {
    pub source_id: String,
    /// Path of the skill inside the source, `/`-separated; empty for roots.
    pub subpath: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_commit: Option<String>,
}

/// A declared tool/MCP dependency of one skill. The dependency state
/// (bound / pending configuration / target unsupported) is derived at read
/// time from the link and the deployment targets; nothing is guessed from
/// prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillDependency {
    /// Stable name shown to the user, e.g. the MCP server key expected.
    pub name: String,
    /// Linked library MCP definition, when the user associated one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_id: Option<String>,
}

/// A compatibility diagnostic attached to a skill definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompatibilityNote {
    /// Stable machine code, e.g. `host-scoped-frontmatter`.
    pub code: String,
    pub message: String,
}

/// The skill half of an extension definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillDefinition {
    /// Digest of the library content version this definition deploys.
    pub content_digest: String,
    pub manifest: SkillManifest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceRef>,
    /// Set when the content only satisfies one client's native loading
    /// rules; deploying to another client requires a portable local copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_scoped: Option<AppKind>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub compatibility: Vec<CompatibilityNote>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<SkillDependency>,
}
