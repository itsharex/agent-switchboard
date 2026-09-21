use super::*;

pub(super) fn validate_references(
    state: &crate::local_state::LocalState,
    candidate: &CodexProviderFile,
) -> Result<(), CommandError> {
    if let Some(route) = &candidate.profile.subagent_route {
        if route.profile_id == candidate.profile.id {
            validate_target(route, candidate)?;
        } else {
            validate_subagent_route_reference(state, route)?;
        }
    }
    for record in state.configuration().list_codex_providers().map_err(store_error)? {
        if record.profile.id == candidate.profile.id { continue; }
        if let Some(route) = record.profile.subagent_route.as_ref()
            .filter(|route| route.profile_id == candidate.profile.id)
        {
            validate_target(route, candidate)?;
        }
    }
    Ok(())
}

pub(super) fn validate_target(
    route: &asb_core::contracts::CodexSubagentRoute,
    file: &CodexProviderFile,
) -> Result<(), CommandError> {
    crate::gateway::validate_subagent_target(route, file)
        .map(|_| ())
        .map_err(|error| CommandError::new("codex-subagent-route-unresolved", error))
}

pub(super) fn affected_active_profile(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    candidate: &CodexProviderFile,
) -> Result<Option<CodexProviderFile>, CommandError> {
    let active_id = crate::commands::config_status_report(state, gateway)?
        .into_iter().find(|status| status.app == asb_core::AppKind::Codex)
        .and_then(|status| status.active_profile_id);
    let Some(active_id) = active_id else { return Ok(None) };
    if active_id == candidate.profile.id { return Ok(Some(candidate.clone())); }
    let referenced = state.configuration().list_codex_providers().map_err(store_error)?
        .into_iter().find(|record| record.profile.id == active_id)
        .and_then(|record| record.profile.subagent_route)
        .is_some_and(|route| route.profile_id == candidate.profile.id);
    if !referenced {
        return Ok(None);
    }
    state.configuration().find_codex_provider_file(&active_id).map(Some).map_err(store_error)
}

pub(super) fn projection(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
    owner: &CodexProviderFile,
    candidate: &CodexProviderFile,
) -> Result<crate::gateway::GatewayProjection, CommandError> {
    let settings = crate::codex_common::resolve(state, &owner.profile.id)
        .map_err(|error| CommandError::new("codex-common-config-invalid", error))?;
    gateway.project_codex(owner, settings, Some(candidate))
        .map_err(|error| CommandError::new("gateway-projection-invalid", error))
}
