//! The disable direction: clearing previously-written skill rules from
//! the documents and writing new ones for a binding going disabled.

//! Enable/disable rule bookkeeping: the Codex skills.config entries and
//! Claude skillOverrides members that accompany a binding's desired
//! state, restored from their baselines when the intent flips.

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    ExtensionBinding, ExtensionDefinition, ExtensionPayload, ExtensionTarget, ManagedBaseline,
    ManagedBaselineFile,
};
use asb_core::extensions::mcp::{
    apply_claude_project_disabled_members, apply_claude_skill_overrides,
    apply_claude_user_server_patches, apply_codex_server_patches, apply_codex_skill_rules,
    claude_project_disabled_member_present, render_codex, SkillOverrideValue,
};

use crate::commands::error::CommandError;
use crate::commands::extensions::support::*;

use super::super::{McpScope, Planner};

impl Planner<'_> {
    pub(super) fn skill_rule_path(
        &self,
        binding: &ExtensionBinding,
    ) -> Result<String, CommandError> {
        // The Codex rule matches the canonical document path of the skill:
        // the deployed directory's SKILL.md.
        let target_dir = self.skill_target_dir(binding)?;
        Ok(target_dir.join("SKILL.md").to_string_lossy().to_string())
    }

    #[allow(clippy::type_complexity)]
    pub(crate) fn disable_steps(
        &self,
        definition: &ExtensionDefinition,
        binding: &ExtensionBinding,
        shared_settings: Option<bool>,
        batch: &mut super::super::document::DocumentBatch,
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
                // One resolution for every operation: the same typed target
                // the install path uses, so a project-shared Codex binding
                // disables in its own document, never in another client's.
                let target = self.mcp_document(binding)?;
                let document = target.path.clone();
                if !self.document_baseline_is_current(binding, &document)? {
                    return Err(CommandError::keyed(
                        "extension-baseline",
                        "errors.extops.mcpBaselineMissingForDisable",
                        "MCP 绑定缺少可验证的部署基线，不能安全停用",
                    ));
                }
                let key = binding.native_key.as_deref().unwrap_or_default();
                let work = self.document_work(
                    batch,
                    &document,
                    binding.target.client(),
                    target.syntax,
                )?;
                let base = work.rendered.clone();
                match &target.scope {
                    McpScope::CodexServers => {
                        let render =
                            render_codex(mcp, false, self.secrets).map_err(projection_error)?;
                        let (rendered, changes) =
                            apply_codex_server_patches(&base, &[(key.to_string(), Some(render))])
                                .map_err(adapter_error)?;
                        if !changes.is_empty() {
                            work.claim(&format!("mcp_servers.{key}"), writer)?;
                            work.rendered = rendered;
                        }
                        let original_before =
                            changes.first().and_then(|change| change.before.clone());
                        let written = changes.first().and_then(|change| change.after.clone());
                        Ok((
                            changes,
                            Some(self.merge_baseline_entry(
                                &binding.id,
                                ManagedBaseline::DocumentEntry {
                                    target_path: document.to_string_lossy().to_string(),
                                    entry_pointer: format!("mcp_servers.{key}"),
                                    original_value: original_before,
                                    last_written_value: written,
                                    last_document_hash: sha_hex(work.rendered.as_bytes()),
                                    target_existed_before: work.expected_existed,
                                    backup_reference: None,
                                },
                            )?),
                            Vec::new(),
                        ))
                    }
                    McpScope::ClaudeUserServers => {
                        let (rendered, changes) =
                            apply_claude_user_server_patches(&base, &[(key.to_string(), None)])
                                .map_err(adapter_error)?;
                        if !changes.is_empty() {
                            work.claim(&format!("mcpServers.{key}"), writer)?;
                            work.rendered = rendered;
                        }
                        let original_before =
                            changes.first().and_then(|change| change.before.clone());
                        Ok((
                            changes,
                            Some(self.merge_baseline_entry(
                                &binding.id,
                                ManagedBaseline::DocumentEntry {
                                    target_path: document.to_string_lossy().to_string(),
                                    entry_pointer: format!("mcpServers.{key}"),
                                    original_value: original_before,
                                    last_written_value: None,
                                    last_document_hash: sha_hex(work.rendered.as_bytes()),
                                    target_existed_before: work.expected_existed,
                                    backup_reference: None,
                                },
                            )?),
                            vec!["将从 Claude 配置移除该服务条目，库中保留定义".to_string()],
                        ))
                    }
                    McpScope::ClaudeProjectPrivate { project_path } => {
                        let original_present =
                            claude_project_disabled_member_present(&base, project_path, key)
                                .map_err(adapter_error)?;
                        let (rendered, changes) = apply_claude_project_disabled_members(
                            &base,
                            project_path,
                            &[key.to_string()],
                            &[],
                        )
                        .map_err(adapter_error)?;
                        if changes.is_empty() {
                            return Ok((
                                Vec::new(),
                                None,
                                vec!["该项目已由原生 MCP 停用规则限制；本应用不会接管该规则"
                                    .to_string()],
                            ));
                        }
                        work.claim(
                            &format!("projects.{project_path}.disabledMcpServers#{key}"),
                            writer,
                        )?;
                        work.rendered = rendered;
                        Ok((
                            changes,
                            Some(self.merge_baseline_entry(
                                &binding.id,
                                ManagedBaseline::SetMember {
                                    target_path: document.to_string_lossy().to_string(),
                                    collection_pointer: format!(
                                        "projects.{project_path}.disabledMcpServers"
                                    ),
                                    member: key.to_string(),
                                    original_present,
                                    last_present: true,
                                    last_document_hash: sha_hex(work.rendered.as_bytes()),
                                    target_existed_before: work.expected_existed,
                                    backup_reference: None,
                                },
                            )?),
                            vec!["仅在该项目停用；其他项目仍可使用同名服务".to_string()],
                        ))
                    }
                }
            }
            ExtensionPayload::Skill(_) => {
                self.disable_skill_steps(definition, binding, shared_settings, batch, writer)
            }
        }
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn disable_skill_steps(
        &self,
        definition: &ExtensionDefinition,
        binding: &ExtensionBinding,
        shared_settings: Option<bool>,
        batch: &mut super::super::document::DocumentBatch,
        writer: usize,
    ) -> Result<
        (
            Vec<asb_core::extensions::mcp::EntryChange>,
            Option<ManagedBaselineFile>,
            Vec<String>,
        ),
        CommandError,
    > {
        let ExtensionPayload::Skill(_) = &definition.payload else {
            return Err(CommandError::keyed(
                "extension-invalid",
                "errors.extops.notSkillDefinition",
                "不是 Skill 定义",
            ));
        };
        let name = binding
            .deploy_name
            .as_deref()
            .ok_or_else(|| {
                CommandError::keyed(
                    "extension-invalid",
                    "errors.extops.skillBindingMissingDeployName",
                    "Skill 绑定缺少部署名",
                )
            })?;
        match binding.target.client() {
            AppKind::Codex => {
                if shared_settings.is_some() {
                    return Err(CommandError::keyed(
                        "extension-invalid",
                        "errors.extops.codexSkillDisableNoClaudeScope",
                        "Codex Skill 停用不接受 Claude 设置作用域",
                    ));
                }
                let document = crate::extensions::paths::codex_config_path(
                    &self.paths.home,
                    self.paths.codex_home.as_deref(),
                );
                self.require_owned_document_baseline_current(binding, &document)?;
                let rule_path = self.skill_rule_path(binding)?;
                let work = self.document_work(
                    batch,
                    &document,
                    AppKind::Codex,
                    asb_core::extensions::contracts::DocumentSyntax::Toml,
                )?;
                let (rendered, changes) = apply_codex_skill_rules(
                    &work.rendered.clone(),
                    &[(rule_path.clone(), Some(false))],
                )
                .map_err(adapter_error)?;
                if !changes.is_empty() {
                    work.claim(&format!("skills.config[path={rule_path}]"), writer)?;
                    work.rendered = rendered;
                }
                let original_before = changes.first().and_then(|change| change.before.clone());
                let written = changes.first().and_then(|change| change.after.clone());
                Ok((
                    changes,
                    Some(self.merge_baseline_entry(
                        &binding.id,
                        ManagedBaseline::DocumentEntry {
                            target_path: document.to_string_lossy().to_string(),
                            entry_pointer: format!("skills.config[path={rule_path}]"),
                            original_value: original_before,
                            last_written_value: written,
                            last_document_hash: sha_hex(work.rendered.as_bytes()),
                            target_existed_before: work.expected_existed,
                            backup_reference: None,
                        },
                    )?),
                    vec!["Skill 文件保留，仅写入原生停用规则".to_string()],
                ))
            }
            AppKind::Claude => {
                let document = match &binding.target {
                    ExtensionTarget::App { .. } => {
                        if shared_settings.is_some() {
                            return Err(CommandError::keyed(
                                "extension-invalid",
                                "errors.extops.userSkillDisableNoProjectScope",
                                "用户级 Claude Skill 停用不接受项目设置作用域",
                            ));
                        }
                        crate::extensions::paths::claude_settings_path(
                            &self.paths.home,
                            self.paths.claude_dir.as_deref(),
                        )
                    }
                    ExtensionTarget::ProjectShared { .. } => {
                        let shared = shared_settings.ok_or_else(|| {
                            CommandError::keyed(
                                "extension-invalid",
                                "errors.extops.projectSkillDisableScopeRequired",
                                "请明确选择将 Claude 项目 Skill 停用写入共享设置还是个人设置",
                            )
                        })?;
                        let root = self.project_path(binding)?;
                        if shared {
                            crate::extensions::paths::claude_project_settings_path(
                                std::path::Path::new(&root),
                            )
                        } else {
                            crate::extensions::paths::claude_project_local_settings_path(
                                std::path::Path::new(&root),
                            )
                        }
                    }
                    ExtensionTarget::ProjectPrivate { .. } => {
                        if shared_settings.is_some() {
                            return Err(CommandError::keyed(
                                "extension-invalid",
                                "errors.extops.privateSkillDisableNoSharedScope",
                                "项目私有 Claude Skill 停用不接受共享设置选择",
                            ));
                        }
                        let root = self.project_path(binding)?;
                        crate::extensions::paths::claude_project_local_settings_path(
                            std::path::Path::new(&root),
                        )
                    }
                };
                self.require_owned_document_baseline_current(binding, &document)?;
                let work = self.document_work(
                    batch,
                    &document,
                    AppKind::Claude,
                    asb_core::extensions::contracts::DocumentSyntax::Json,
                )?;
                let (rendered, changes) = apply_claude_skill_overrides(
                    &work.rendered.clone(),
                    &[(name.to_string(), Some(SkillOverrideValue::Off))],
                )
                .map_err(adapter_error)?;
                if !changes.is_empty() {
                    work.claim(&format!("skillOverrides.{name}"), writer)?;
                    work.rendered = rendered;
                }
                let mut warnings = vec![format!(
                    "skillOverrides 按名字生效；同名不同来源的 {name} 也会被停用"
                )];
                if matches!(binding.target, ExtensionTarget::ProjectShared { .. })
                    && shared_settings == Some(true)
                {
                    warnings.push("该项目共享设置文件可能被版本控制；请确认提交范围".to_string());
                }
                let original_before = changes.first().and_then(|change| change.before.clone());
                let written = changes.first().and_then(|change| change.after.clone());
                Ok((
                    changes,
                    Some(self.merge_baseline_entry(
                        &binding.id,
                        ManagedBaseline::DocumentEntry {
                            target_path: document.to_string_lossy().to_string(),
                            entry_pointer: format!("skillOverrides.{name}"),
                            original_value: original_before,
                            last_written_value: written,
                            last_document_hash: sha_hex(work.rendered.as_bytes()),
                            target_existed_before: work.expected_existed,
                            backup_reference: None,
                        },
                    )?),
                    warnings,
                ))
            }
        }
    }
}
