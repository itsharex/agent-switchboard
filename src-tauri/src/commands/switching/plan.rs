use crate::commands::error::{store_error, CommandError};
use asb_core::contracts::{AppKind, ProviderProfile, SwitchPlan};
use asb_switch::io::FsIo;
use asb_switch::{read_codex_preview, read_preview};

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
    let common = state
        .configuration()
        .get_common_settings(profile.app)
        .map_err(store_error)?
        .settings;
    let plan = SwitchPlan::direct(profile, common);
    asb_core::validate_plan(&plan.profile, &plan.common)
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
    let mut preview = match plan.app() {
        AppKind::Codex => {
            let auth_target = crate::local_state::LocalState::codex_auth_path()
                .map_err(|error| CommandError::new("codex-auth-path-unavailable", error))?;
            read_codex_preview(
                &FsIo,
                &target,
                &auth_target,
                plan,
                &backup_dir.to_string_lossy(),
            )
        }
        AppKind::Claude => read_preview(&FsIo, &target, plan, &backup_dir.to_string_lossy()),
    }
    .map_err(CommandError::from)?;
    if let Some(warning) = projection.warning() {
        preview.preview.warnings.push(warning.to_string());
    }
    preview.preview.target = target.to_string_lossy().to_string();
    Ok(preview)
}
