//! Codex identity and visual-config matching; no Claude business rules here.
use crate::{
    commands::error::{store_error, CommandError},
    local_state::LocalState,
};
use asb_core::{
    contracts::{ProviderProfile, ProviderRecord, SwitchPlan},
    AppKind,
};
fn error(message: String) -> CommandError {
    CommandError::new("codex-state-unavailable", message)
}
fn settings(state: &LocalState, id: &str) -> Result<asb_core::SettingsValues, CommandError> {
    crate::codex_common::resolve(state, id).map_err(error)
}
/// Status matching replays the full projection, so every matching plan must
/// carry the common-file fragment exactly like a switching plan would.
fn attach_fragment(
    state: &LocalState,
    profile_id: &str,
    plan: SwitchPlan,
) -> Result<SwitchPlan, CommandError> {
    Ok(
        match crate::codex_common::resolve_fragment(state.root(), profile_id).map_err(error)? {
            Some(fragment) => plan.with_codex_common_fragment(fragment),
            None => plan,
        },
    )
}
pub(super) fn matching_profile(
    state: &LocalState,
    gateway: Option<&crate::gateway::GatewayController>,
    text: &str,
) -> Result<Option<ProviderProfile>, CommandError> {
    if let Some(gateway) = gateway {
        if let Some(id) = gateway
            .active_profile_id(AppKind::Codex, text)
            .map_err(error)?
        {
            let file = state
                .configuration()
                .find_codex_provider_file(&id)
                .map_err(store_error)?;
            let plan = gateway
                .active_codex_projection(&file, settings(state, &id)?)
                .map_err(error)?;
            return Ok(plan
                .filter(|plan| super::report::projection_matches(text, plan))
                .map(|_| file.client_projection().into_profile(AppKind::Codex)));
        }
    }
    let mut matches = Vec::new();
    for record in state
        .configuration()
        .list_codex_providers()
        .map_err(store_error)?
    {
        if record.profile.route_mode != asb_core::contracts::CodexRouteMode::Direct {
            continue;
        }
        let file = state
            .configuration()
            .find_codex_provider_file(&record.profile.id)
            .map_err(store_error)?;
        let profile = file.client_projection().into_profile(AppKind::Codex);
        let revision = crate::gateway::codex_route_fingerprint(&file).map_err(error)?;
        let plan = attach_fragment(
            state,
            &file.profile.id,
            SwitchPlan::direct(profile.clone(), settings(state, &file.profile.id)?)
                .with_codex_model_catalog(crate::gateway::codex_catalog_file_name(
                    &file.profile.id,
                    &revision,
                    &file.profile.catalog,
                )),
        )?;
        if super::report::projection_matches(text, &plan) {
            matches.push(profile);
        }
    }
    Ok(if matches.len() == 1 {
        matches.pop()
    } else {
        None
    })
}
pub(super) fn matching_official(
    state: &LocalState,
    records: &[ProviderRecord],
    text: &str,
) -> Result<Option<ProviderProfile>, CommandError> {
    for record in records {
        if record.profile.route_mode != asb_core::RouteMode::Official
            || !account_matches(state, &record.profile.id)?
        {
            continue;
        }
        let plan = attach_fragment(
            state,
            &record.profile.id,
            SwitchPlan::direct(record.profile.clone(), settings(state, &record.profile.id)?),
        )?;
        if super::report::projection_matches(text, &plan) {
            return Ok(Some(record.profile.clone()));
        }
    }
    Ok(None)
}
pub(super) fn account_matches(state: &LocalState, id: &str) -> Result<bool, CommandError> {
    crate::codex_auth::projection::matches_binding(state, id).map_err(error)
}
pub(crate) fn active_direct(
    state: &LocalState,
    text: &str,
) -> Result<Option<String>, CommandError> {
    let document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| error("Codex 配置格式无效".into()))?;
    let pointer = document
        .get("model_catalog_json")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let name = std::path::Path::new(pointer)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");
    let endpoint = asb_core::adapter::route_state(AppKind::Codex, text).base_url;
    if endpoint
        .as_deref()
        .is_none_or(asb_core::adapter::codex::is_gateway_base_url)
    {
        return Ok(None);
    }
    let target = state
        .target(AppKind::Codex)
        .map_err(error)?
        .with_file_name("auth.json");
    let auth = match std::fs::read_to_string(target) {
        Ok(text) => serde_json::from_str::<serde_json::Value>(&text).ok(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(error("无法读取 Codex 直连认证状态".into())),
    };
    let key = auth
        .as_ref()
        .and_then(|v| v.get("OPENAI_API_KEY"))
        .and_then(|v| v.as_str());
    for record in state
        .configuration()
        .list_codex_providers()
        .map_err(store_error)?
    {
        let profile = record.profile;
        if profile.route_mode == asb_core::contracts::CodexRouteMode::Direct
            && endpoint.as_deref() == Some(profile.endpoint.0.as_str())
            && key == Some(profile.api_key.as_str())
            && name.starts_with(&format!("agent-switchboard-codex-{}-", profile.id))
            && name.ends_with(".json")
        {
            return Ok(Some(profile.id));
        }
    }
    Ok(None)
}
