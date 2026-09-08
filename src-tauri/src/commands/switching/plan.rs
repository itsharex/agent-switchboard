use crate::commands::error::{store_error, CommandError};
use asb_core::contracts::{AppKind, ProviderProfile, SwitchPlan};
use asb_switch::io::FsIo;
use asb_switch::read_preview;

pub(super) fn build_plan(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    profile_id: &str,
) -> Result<crate::gateway::GatewayProjection, CommandError> {
    let configuration = state.configuration();
    let profile = configuration
        .find_provider(profile_id)
        .map_err(|error| CommandError::new("profile-not-found", error))?;
    build_plan_for_profile(state, gateway, profile)
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
    super::transaction::begin(
        state,
        gateway,
        plan.app(),
        Some(&plan.profile.id),
        expected_rendered_hash,
        true,
    )?;
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
                        profile_id: Some(plan.profile.id.clone()),
                        profile_name: Some(plan.profile.name.clone()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::{RouteMode, UpstreamProtocol};

    #[test]
    fn third_party_preflight_requires_official_login_without_writing_config() {
        let _paths = crate::test_client_paths::redirect_client_paths();
        let directory = tempfile::tempdir().unwrap();
        let state = crate::local_state::LocalState::from_root(directory.path().join("state"));
        let gateway = crate::gateway::GatewayController::start(&state);
        let target = state.target(AppKind::Codex).unwrap();
        let profile = ProviderProfile {
            id: "login-preflight".into(),
            app: AppKind::Codex,
            route_mode: RouteMode::Custom,
            name: "relay".into(),
            base_url: Some("https://relay.example/v1".into()),
            api_key: "fixture-key".into(),
            upstream_protocol: Some(UpstreamProtocol::Responses),
            responses_options: Some(asb_core::contracts::ResponsesOptions {
                request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
            }),
            model: Some("fixture-model".into()),
            model_options: None,
            max_output_tokens: None.into(),
            parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        };
        assert!(build_plan_for_profile(&state, &gateway, profile.clone()).is_err());
        assert!(!target.exists());
        let auth = target.parent().unwrap().join("auth.json");
        for value in [
            "broken",
            r#"{"auth_mode":"apikey","tokens":{"access_token":"residue"}}"#,
        ] {
            std::fs::write(&auth, value).unwrap();
            assert!(build_plan_for_profile(&state, &gateway, profile.clone()).is_err());
            assert_eq!(std::fs::read_to_string(&auth).unwrap(), value);
            assert!(!target.exists());
        }
        let official = r#"{"auth_mode":"chatgpt","tokens":{"access_token":"at","refresh_token":"rt","id_token":"id"}}"#;
        std::fs::write(&auth, official).unwrap();
        let projection = build_plan_for_profile(&state, &gateway, profile).unwrap();
        assert!(projection.plan.is_gateway());
        let preview = preview_projection(&state, &projection).unwrap();
        assert!(!preview.content.contains("fixture-key"));
        assert!(!target.exists());
        assert_eq!(std::fs::read_to_string(&auth).unwrap(), official);
        gateway.shutdown();
    }
}
