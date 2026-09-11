use crate::commands::error::{store_error, CommandError};
use asb_core::contracts::{AppKind, ProviderProfile, SwitchPlan};
use asb_switch::io::FsIo;
use asb_switch::read_preview;
use std::fs;
use std::path::Path;

/// Official Codex is not a third-party profile: switching to it targets the
/// stored official-login record, whose direct plan is built by
/// `build_plan_for_profile` exactly like any other profile.
pub(super) fn build_plan(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    profile_id: &str,
) -> Result<crate::gateway::GatewayProjection, CommandError> {
    let configuration = state.configuration();
    if let Ok(file) = configuration.find_codex_provider_file(profile_id) {
        return build_codex_plan(state, gateway, file);
    }
    let profile = configuration
        .find_provider(profile_id)
        .map_err(|error| CommandError::new("profile-not-found", error))?;
    build_plan_for_profile(state, gateway, profile)
}

pub(super) fn build_codex_plan(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    file: asb_core::contracts::CodexProviderFile,
) -> Result<crate::gateway::GatewayProjection, CommandError> {
    let target = state
        .target(AppKind::Codex)
        .map_err(|error| CommandError::new("config-path-unavailable", error))?;
    crate::official_login::observation::require_codex_login(&target)
        .map_err(|error| CommandError::new("codex-official-login-required", error))?;
    let client_settings = state
        .configuration()
        .get_client_settings(AppKind::Codex)
        .map_err(store_error)?
        .settings;
    gateway
        .project_codex(&file, client_settings)
        .map_err(|error| CommandError::new("gateway-projection-invalid", error))
}

pub(super) fn build_plan_for_profile(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    profile: ProviderProfile,
) -> Result<crate::gateway::GatewayProjection, CommandError> {
    let client_settings = state
        .configuration()
        .get_client_settings(profile.app)
        .map_err(store_error)?
        .settings;
    if profile.app == AppKind::Codex && profile.route_mode == asb_core::RouteMode::Custom {
        let target = state
            .target(AppKind::Codex)
            .map_err(|error| CommandError::new("config-path-unavailable", error))?;
        crate::official_login::observation::require_codex_login(&target)
            .map_err(|error| CommandError::new("codex-official-login-required", error))?;
    }
    let plan = SwitchPlan::direct(profile, client_settings);
    asb_core::validate_plan(&plan.profile, &plan.client_settings)
        .map_err(|error| CommandError::new("invalid-plan", error.to_string()))?;
    gateway
        .project(&plan)
        .map_err(|error| CommandError::new("gateway-projection-invalid", error))
}

pub(super) fn preview_projection(
    state: &crate::local_state::LocalState,
    projection: &crate::gateway::GatewayProjection,
) -> Result<asb_switch::FilePreview, CommandError> {
    let plan = &projection.plan;
    let target = state
        .target(plan.app())
        .map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let backup_dir = state.backup_dir();
    let mut preview = read_preview(&FsIo, &target, plan, &backup_dir.to_string_lossy())
        .map_err(CommandError::from)?;
    if let Some(warning) = projection.warning() {
        preview.preview.warnings.push(warning.to_string());
    }
    if let Some(catalog) = &projection.codex_catalog {
        preview.preview.warnings.push(format!(
            "将生成并写入 ASB 管理的 Codex 模型目录 {}，并由 model_catalog_json 指向该文件。",
            catalog.file_name
        ));
    } else if plan.app() == AppKind::Codex
        && plan.profile.route_mode == asb_core::RouteMode::Official
        && official_catalog_artifact(&target)?.is_some()
    {
        preview.preview.warnings.push(
            "将清理当前 model_catalog_json 指向的 ASB 管理模型目录；用户自己的目录文件不会被删除。"
                .to_string(),
        );
    }
    preview.preview.target = target.to_string_lossy().to_string();
    Ok(preview)
}

pub(super) fn execute_projection(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    projection: &crate::gateway::GatewayProjection,
    expected_hash: &str,
    expected_rendered_hash: &str,
) -> Result<asb_switch::SwitchOutcome, CommandError> {
    let plan = &projection.plan;
    let target = state
        .target(plan.app())
        .map_err(|error| CommandError::new("config-path-unavailable", error))?;
    if plan.app() == AppKind::Codex && plan.profile.route_mode == asb_core::RouteMode::Custom {
        crate::official_login::observation::require_codex_login(&target)
            .map_err(|error| CommandError::new("codex-official-login-required", error))?;
    }
    let catalog = match (plan.app(), plan.profile.route_mode) {
        (AppKind::Codex, asb_core::RouteMode::Custom) => catalog_artifact(&target, projection)?,
        (AppKind::Codex, asb_core::RouteMode::Official) => official_catalog_artifact(&target)?,
        (AppKind::Claude, _) => None,
    };
    let profile_id = (plan.profile.route_mode == asb_core::RouteMode::Custom)
        .then_some(plan.profile.id.as_str());
    super::transaction::begin(
        state,
        gateway,
        plan.app(),
        profile_id,
        expected_rendered_hash,
        true,
        catalog.clone(),
    )?;
    if let Some(catalog) = catalog.as_ref() {
        if let Err(error) = super::transaction::apply_catalog_artifact(catalog) {
            super::transaction::recover(state, gateway)
                .map_err(|recovery| CommandError::new("config-recovery-required", recovery))?;
            return Err(error);
        }
    }
    let execution = asb_switch::execute(
        &FsIo,
        &asb_switch::SwitchRequest {
            target: &target,
            plan,
            backup_dir: &state.backup_dir(),
            expected_hash,
            expected_rendered_hash,
        },
        |outcome| {
            gateway.commit(projection, || {
                state
                    .configuration()
                    .record_config_write(asb_core::ConfigWriteRecord {
                        app: plan.app(),
                        profile_id: profile_id.map(str::to_string),
                        profile_name: profile_id.map(|_| plan.profile.name.clone()),
                        content_hash: outcome.final_hash.clone(),
                        backup_id: outcome.backup.id.clone(),
                        at: outcome.backup.created_at.clone(),
                        operation: asb_core::WriteOperation::Projection,
                    })
            })
        },
    );
    let mut outcome = super::transaction::finish(state, gateway, execution)?;
    outcome.preview.target = target.to_string_lossy().into_owned();
    Ok(outcome)
}

fn official_catalog_artifact(
    config_target: &Path,
) -> Result<Option<super::transaction::CatalogArtifact>, CommandError> {
    let text = match fs::read_to_string(config_target) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => {
            return Err(CommandError::new(
                "codex-catalog-unreadable",
                "无法读取 Codex 配置以清理模型目录",
            ));
        }
    };
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| CommandError::new("codex-catalog-pointer-invalid", "Codex 配置无法解析"))?;
    let Some(pointer) = document
        .get("model_catalog_json")
        .and_then(|item| item.as_str())
    else {
        return Ok(None);
    };
    let pointer_path = Path::new(pointer);
    let valid_name = pointer_path
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| name.starts_with("agent-switchboard-codex-") && name.ends_with(".json"));
    if valid_name.is_none() || pointer_path.components().count() != 1 {
        return Ok(None);
    }
    let directory = config_target
        .parent()
        .ok_or_else(|| CommandError::new("codex-catalog-path-invalid", "Codex 配置目录无效"))?;
    let path = directory.join(pointer);
    let before = match fs::read_to_string(&path) {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Err(CommandError::new(
                "codex-catalog-unreadable",
                "无法读取 ASB 管理的 Codex 模型目录",
            ));
        }
    };
    Ok(Some(super::transaction::CatalogArtifact::new(
        path, before, None,
    )))
}

fn catalog_artifact(
    config_target: &Path,
    projection: &crate::gateway::GatewayProjection,
) -> Result<Option<super::transaction::CatalogArtifact>, CommandError> {
    let Some(catalog) = &projection.codex_catalog else {
        return Ok(None);
    };
    let directory = config_target
        .parent()
        .ok_or_else(|| CommandError::new("codex-catalog-path-invalid", "Codex 配置目录无效"))?;
    let path = directory.join(&catalog.file_name);
    let before = match fs::read_to_string(&path) {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => {
            return Err(CommandError::new(
                "codex-catalog-unreadable",
                "无法读取 Codex 模型目录文件",
            ));
        }
    };
    if before
        .as_deref()
        .is_some_and(|content| content != catalog.content)
    {
        return Err(CommandError::new(
            "codex-catalog-revision-conflict",
            "同一 Codex 路由修订的模型目录内容不一致，拒绝覆盖",
        ));
    }
    Ok(Some(super::transaction::CatalogArtifact::new(
        path,
        before,
        Some(catalog.content.clone()),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::contracts::{
        CodexCapabilities, CodexCatalogEntry, CodexEndpoint, CodexModelRoute, CodexProviderDraft,
        CodexUpstream,
    };

    fn codex_file() -> asb_core::contracts::CodexProviderFile {
        CodexProviderDraft {
            name: "relay".to_string(),
            endpoint: CodexEndpoint("https://relay.example/v1".to_string()),
            api_key: "fixture-key".to_string(),
            upstream: CodexUpstream::Responses,
            request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
            default_model: "fixture-model".to_string(),
            catalog: vec![CodexCatalogEntry {
                id: "fixture-model".to_string(),
                context_window: 128_000,
                max_output_tokens: 16_384,
                function_tools: true,
                custom_tools: true,
                tool_search: true,
                reasoning: true,
                default_reasoning_level: asb_core::contracts::CodexReasoningLevel::High,
                supported_reasoning_levels: vec![
                    asb_core::contracts::CodexReasoningLevel::None,
                    asb_core::contracts::CodexReasoningLevel::High,
                ],
                images: true,
                compact: true,
            }],
            model_routes: vec![CodexModelRoute {
                client_model: "fixture-model".to_string(),
                upstream_model: "vendor-model".to_string(),
            }],
            capabilities: CodexCapabilities {
                responses: true,
                compact: true,
                models: true,
                chat_completions: true,
                alpha_search: false,
                image_generation: false,
                image_edit: false,
                function_tools: true,
                custom_tools: true,
                tool_search: true,
                reasoning: true,
                chat_reasoning: asb_core::contracts::CodexChatReasoning::Unsupported,
            },
            parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
            notes: None,
            website_url: None,
            usage_query: None,
        }
        .into_file("00000000-0000-0000-0000-000000000001".to_string(), 100)
    }

    #[test]
    fn third_party_preflight_requires_official_login_without_writing_config() {
        let _paths = crate::test_client_paths::redirect_client_paths();
        let directory = tempfile::tempdir().unwrap();
        let state = crate::local_state::LocalState::from_root(directory.path().join("state"));
        let gateway = crate::gateway::GatewayController::start(&state);
        let target = state.target(AppKind::Codex).unwrap();
        let file = codex_file();
        assert!(build_codex_plan(&state, &gateway, file.clone()).is_err());
        assert!(!target.exists());
        let auth = target.parent().unwrap().join("auth.json");
        for value in [
            "broken",
            r#"{"auth_mode":"apikey","tokens":{"access_token":"residue"}}"#,
        ] {
            std::fs::write(&auth, value).unwrap();
            assert!(build_codex_plan(&state, &gateway, file.clone()).is_err());
            assert_eq!(std::fs::read_to_string(&auth).unwrap(), value);
            assert!(!target.exists());
        }
        let official = r#"{"auth_mode":"chatgpt","tokens":{"access_token":"at","refresh_token":"rt","id_token":"id"}}"#;
        std::fs::write(&auth, official).unwrap();
        let projection = build_codex_plan(&state, &gateway, file).unwrap();
        assert!(projection.plan.is_gateway());
        let preview = preview_projection(&state, &projection).unwrap();
        assert!(!preview.content.contains("fixture-key"));
        assert!(!target.exists());
        assert_eq!(std::fs::read_to_string(&auth).unwrap(), official);
        gateway.shutdown();
    }

    #[test]
    fn official_cleanup_only_targets_the_asb_catalog_pointer() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("config.toml");
        let owned = directory.path().join("agent-switchboard-codex-a.json");
        std::fs::write(&owned, "catalog").unwrap();
        std::fs::write(
            &target,
            "model_catalog_json = \"agent-switchboard-codex-a.json\"\n",
        )
        .unwrap();
        let artifact = official_catalog_artifact(&target).unwrap().unwrap();
        crate::commands::switching::transaction::apply_catalog_artifact(&artifact).unwrap();
        assert!(!owned.exists());

        let user_catalog = directory.path().join("my-models.json");
        std::fs::write(&user_catalog, "user catalog").unwrap();
        std::fs::write(&target, "model_catalog_json = \"my-models.json\"\n").unwrap();
        assert!(official_catalog_artifact(&target).unwrap().is_none());
        assert_eq!(
            std::fs::read_to_string(&user_catalog).unwrap(),
            "user catalog"
        );
    }
}
