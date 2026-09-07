//! The extensions workspace contracts: Skills and MCP servers as managed
//! resources with their own definitions, target bindings, plans, and
//! transactions. This module owns the vocabulary shared by the desktop
//! store, the switch executor, and the frontend projection; it stays free
//! of filesystem and network access.

pub mod contracts;
pub mod content_integrity;
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
                "用户级 Skills：~/.agents/skills/<name>/SKILL.md（原生共享目录，其他工具也可能读取）",
                true,
                open("以选定 Codex 发行版在隔离目录实测清单加载"),
            ));
            entries.push(entry(
                "skill-project",
                "项目 Skills：<project>/.agents/skills/<name>/SKILL.md（含 CWD 相关发现层级）",
                true,
                open("以选定 Codex 发行版实测仓库层级发现"),
            ));
            entries.push(entry(
                "skill-toggle",
                "Skill 停用：config.toml 的 [[skills.config]] 路径规则（canonical document path 匹配）",
                true,
                open("以选定 Codex 发行版实测停用后不可调用"),
            ));
            entries.push(entry(
                "mcp-user",
                "用户级 MCP：$CODEX_HOME/config.toml 的 mcp_servers.<key>",
                true,
                open("以选定 Codex 发行版实测服务加载"),
            ));
            entries.push(entry(
                "mcp-project-shared",
                "项目共享 MCP：<project>/.codex/config.toml 的 mcp_servers.<key>",
                true,
                open("以选定 Codex 发行版实测项目信任与服务加载"),
            ));
            entries.push(entry(
                "mcp-disable",
                "MCP 停用：服务条目内的 enabled = false",
                true,
                open("以选定 Codex 发行版实测停用效果"),
            ));
            entries.push(entry(
                "mcp-project-private",
                "项目私有 MCP：Codex 没有私有项目文件格式，不支持",
                false,
                open("Codex 官方提供私有项目文档后重新评估"),
            ));
        }
        AppKind::Claude => {
            entries.push(entry(
                "skill-user",
                "用户级 Skills：~/.claude/skills/<name>/SKILL.md",
                true,
                open("以选定 Claude Code 发行版在隔离目录实测清单加载"),
            ));
            entries.push(entry(
                "skill-project",
                "项目 Skills：<project>/.claude/skills/<name>/SKILL.md（含父级与嵌套发现规则）",
                true,
                open("以选定 Claude Code 发行版实测发现层级"),
            ));
            entries.push(entry(
                "skill-toggle",
                "Skill 可见性：settings 的 skillOverrides.<name>（on / name-only / user-invocable-only / off）",
                true,
                open("以选定 Claude Code 发行版实测四态语义"),
            ));
            if environment.claude_config_dir_custom {
                entries.push(entry(
                    "mcp-user",
                    "用户级 MCP：~/.claude.json 的 mcpServers（自定义 CLAUDE_CONFIG_DIR 下状态文件位置未核实）",
                    false,
                    open("以官方 CLI 在隔离 CLAUDE_CONFIG_DIR 中创建无凭据假服务核实状态文件位置"),
                ));
            } else {
                entries.push(entry(
                    "mcp-user",
                    "用户级 MCP：~/.claude.json 的 mcpServers.<key>",
                    true,
                    open("以选定 Claude Code 发行版实测服务加载"),
                ));
            }
            entries.push(entry(
                "mcp-project-shared",
                "项目共享 MCP：<project>/.mcp.json 的 mcpServers.<key>（受项目信任限制）",
                true,
                open("以选定 Claude Code 发行版实测项目信任交互"),
            ));
            entries.push(entry(
                "mcp-project-private",
                "项目私有 MCP：~/.claude.json 的 projects[<绝对路径>].mcpServers.<key>",
                true,
                open("以选定 Claude Code 发行版实测私有作用域优先级"),
            ));
            entries.push(entry(
                "mcp-disable",
                "项目级停用：projects[<路径>].disabledMcpServers 成员（用户级停用为撤下条目）",
                true,
                open("以选定 Claude Code 发行版实测 /mcp 停用状态对应字段"),
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
pub use diagnostics::{DiagnosticCode, DiagnosticRemediation, DiagnosticSubject, ExtensionDiagnostic};
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
