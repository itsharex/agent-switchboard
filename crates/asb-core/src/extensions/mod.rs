//! The extensions workspace contracts: Skills and MCP servers as managed
//! resources with their own definitions, target bindings, plans, and
//! transactions. This module owns the vocabulary shared by the desktop
//! store, the switch executor, and the frontend projection; it stays free
//! of filesystem and network access.

pub mod content_integrity;
pub mod contracts;
pub mod diagnostics;
pub mod edit;
pub mod mcp;
pub mod plan;
pub mod portable;
pub mod skill;
pub mod validate;

use crate::contracts::AppKind;
use crate::extensions::contracts::{
    CapabilityEntry, CapabilityVerification, ClientCapabilityReport,
};

/// Environment facts that change which capability rules apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CapabilityEnvironment {
    /// The client runs with a custom `CLAUDE_CONFIG_DIR`; the location of
    /// the user MCP state file under such a setup has not been confirmed
    /// against an official release yet.
    pub claude_config_dir_custom: bool,
}

/// The per-client capability report. Path rules follow the current official
/// documentation; verification records name the isolation test that would
/// confirm each rule on a real release. Nothing here claims a tested
/// version until that test has run.
pub fn capability_report(
    client: AppKind,
    environment: &CapabilityEnvironment,
) -> ClientCapabilityReport {
    let open = |condition: &str| CapabilityVerification::Open {
        condition: condition.to_string(),
    };
    let entry = |code: &str, resource: &str, supported: bool, verification| CapabilityEntry {
        code: code.to_string(),
        resource: resource.to_string(),
        supported,
        verification,
    };
    let mut entries = Vec::new();
    match client {
        AppKind::Codex => {
            entries.push(entry(
                "skill-user",
                "extcap.codex.skillUser.resource",
                true,
                open("extcap.codex.skillUser.verification"),
            ));
            entries.push(entry(
                "skill-project",
                "extcap.codex.skillProject.resource",
                true,
                open("extcap.codex.skillProject.verification"),
            ));
            entries.push(entry(
                "skill-toggle",
                "extcap.codex.skillToggle.resource",
                true,
                open("extcap.codex.skillToggle.verification"),
            ));
            entries.push(entry(
                "mcp-user",
                "extcap.codex.mcpUser.resource",
                true,
                open("extcap.codex.mcpUser.verification"),
            ));
            entries.push(entry(
                "mcp-project-shared",
                "extcap.codex.mcpProjectShared.resource",
                true,
                open("extcap.codex.mcpProjectShared.verification"),
            ));
            entries.push(entry(
                "mcp-disable",
                "extcap.codex.mcpDisable.resource",
                true,
                open("extcap.codex.mcpDisable.verification"),
            ));
            entries.push(entry(
                "mcp-project-private",
                "extcap.codex.mcpProjectPrivate.resource",
                false,
                open("extcap.codex.mcpProjectPrivate.verification"),
            ));
        }
        AppKind::Claude => {
            entries.push(entry(
                "skill-user",
                "extcap.claude.skillUser.resource",
                true,
                open("extcap.claude.skillUser.verification"),
            ));
            entries.push(entry(
                "skill-project",
                "extcap.claude.skillProject.resource",
                true,
                open("extcap.claude.skillProject.verification"),
            ));
            entries.push(entry(
                "skill-toggle",
                "extcap.claude.skillToggle.resource",
                true,
                open("extcap.claude.skillToggle.verification"),
            ));
            if environment.claude_config_dir_custom {
                entries.push(entry(
                    "mcp-user",
                    "extcap.claude.mcpUser.resource",
                    false,
                    open("extcap.claude.mcpUser.verification"),
                ));
            } else {
                entries.push(entry(
                    "mcp-user",
                    "extcap.claude.mcpUserStandard.resource",
                    true,
                    open("extcap.claude.mcpUserStandard.verification"),
                ));
            }
            entries.push(entry(
                "mcp-project-shared",
                "extcap.claude.mcpProjectShared.resource",
                true,
                open("extcap.claude.mcpProjectShared.verification"),
            ));
            entries.push(entry(
                "mcp-project-private",
                "extcap.claude.mcpProjectPrivate.resource",
                true,
                open("extcap.claude.mcpProjectPrivate.verification"),
            ));
            entries.push(entry(
                "mcp-disable",
                "extcap.claude.mcpDisable.resource",
                true,
                open("extcap.claude.mcpDisable.verification"),
            ));
        }
    }
    ClientCapabilityReport { client, entries }
}

/// Names every native capability a planned binding needs. The command layer
/// turns these codes into a hard write gate using the report for its current
/// environment; discovery and preview intentionally remain available.
pub fn required_capability_codes(
    kind: ExtensionKind,
    target: &ExtensionTarget,
    changes_visibility: bool,
) -> Vec<&'static str> {
    let primary = match (kind, target) {
        (ExtensionKind::Skill, ExtensionTarget::App { .. }) => "skill-user",
        (ExtensionKind::Skill, ExtensionTarget::ProjectShared { .. })
        | (ExtensionKind::Skill, ExtensionTarget::ProjectPrivate { .. }) => "skill-project",
        (ExtensionKind::Mcp, ExtensionTarget::App { .. }) => "mcp-user",
        (ExtensionKind::Mcp, ExtensionTarget::ProjectShared { .. }) => "mcp-project-shared",
        (ExtensionKind::Mcp, ExtensionTarget::ProjectPrivate { .. }) => "mcp-project-private",
    };
    let mut codes = vec![primary];
    if changes_visibility {
        codes.push(match kind {
            ExtensionKind::Skill => "skill-toggle",
            ExtensionKind::Mcp => "mcp-disable",
        });
    }
    codes
}

#[cfg(test)]
mod capability_tests {
    use super::*;

    #[test]
    fn codex_project_mcp_has_a_write_capability_and_skill_toggle_is_explicit() {
        let report = capability_report(AppKind::Codex, &CapabilityEnvironment::default());
        assert!(report
            .entries
            .iter()
            .any(|entry| entry.code == "mcp-project-shared" && entry.supported));
        let target = ExtensionTarget::ProjectShared {
            client: AppKind::Codex,
            project_id: "project-1".to_string(),
        };
        assert_eq!(
            required_capability_codes(ExtensionKind::Mcp, &target, true),
            vec!["mcp-project-shared", "mcp-disable"]
        );
    }
}

pub use contracts::{
    AppliedExtensionStep, AppliedStep, CodexServerOptions, CompatibilityNote, DesiredState,
    DocumentSyntax, ExtensionBinding, ExtensionDefinition, ExtensionKind, ExtensionManifest,
    ExtensionPayload, ExtensionTarget, FieldEdit, ManagedBaseline, ManagedFileEntry,
    McpCheckOutcome, McpCheckResult, McpDefinition, McpEditRequest, McpFieldEdits,
    ObservedExtension, ObservedOrigin, PlanOperation, ProjectRegistration, RollbackSummary,
    SecretValue, SkillDefinition, SkillDependency, SkillManifest, SourceRef, TargetOutcome,
    EXTENSIONS_SCHEMA_VERSION,
};
pub use diagnostics::{
    DiagnosticCode, DiagnosticRemediation, DiagnosticSubject, ExtensionDiagnostic,
};
pub use edit::{apply_mcp_edit, mcp_edit_view, McpEditView, SecretSlot, SecretSlotView};
pub use mcp::{
    apply_claude_project_disabled_members, apply_claude_project_private_server_patches,
    apply_claude_project_server_patches, apply_claude_skill_override_restore,
    apply_claude_skill_overrides, apply_claude_user_server_patches, apply_codex_entry_restore,
    apply_codex_server_patches, apply_codex_skill_rule_restore, apply_codex_skill_rules,
    read_claude_servers, read_codex_servers, read_codex_skill_rules, render_claude, render_codex,
    McpCollectionProblem, McpEntryProblem, ObservedMcpDocument, ObservedMcpServer,
    ObservedTransport, ProjectionError, SkillOverrideValue,
};
pub use plan::{
    redact_plan, ExtensionPlan, ExtensionPlanView, PlanStale, PlanStep, PlannedFile, PlannedTarget,
    PLAN_TTL_SECONDS,
};
pub use portable::{
    export_mcp, export_skill, portable_dependencies, prepare_import, ImportMaterial, PortableError,
    PortableFile, PortablePackage, PortablePayload, PORTABLE_PACKAGE_VERSION,
};
pub use skill::{
    content_digest, deploy_name_for, extract_manifest, ContentEntry, ManifestExtraction,
};
pub use validate::{
    validate_binding, validate_content_paths, validate_definition, validate_definition_draft,
    validate_server_key, validate_skill_name, validate_target, ExtensionValidationError,
    MAX_CONTENT_DEPTH, MAX_CONTENT_FILES, MAX_CONTENT_FILE_BYTES, MAX_CONTENT_TOTAL_BYTES,
};
