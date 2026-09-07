use crate::adapter::codex::OFFICIAL_PROVIDER;
use crate::adapter::OverlayEntry;
use crate::contracts::{
    AppKind, CommonSettingValue, CommonSettings, ConfigValue, ModelOptions, RouteMode, SwitchPlan,
};
use crate::ownership::{
    provider_absent_action, setting_specs, ProviderAbsentAction, SettingOwner,
    CODEX_LEGACY_PROVIDER_BASE_URL_KEY, CODEX_LEGACY_PROVIDER_ID, CODEX_LEGACY_PROVIDER_NAME_KEY,
    CODEX_LEGACY_PROVIDER_TOKEN_KEY, CODEX_LEGACY_PROVIDER_WIRE_API_KEY, CODEX_WEB_SEARCH_KEY,
};

/// Builds the overlay entries implied by `plan`. Every route uses Codex's
/// built-in `openai` provider, so session history remains in one provider
/// bucket while the selected third-party endpoint and API-key cache vary.
fn absent_provider_entry(key: &str) -> OverlayEntry {
    match provider_absent_action(AppKind::Codex, key) {
        Some(ProviderAbsentAction::Remove) => OverlayEntry::RemoveIfPresent,
        None => unreachable!("Codex provider mapping must be declared in the ownership directory"),
    }
}

fn provider_value(profile: &crate::contracts::ProviderProfile, key: &str) -> Option<ConfigValue> {
    match key {
        "model" => profile.model.clone().map(ConfigValue::Str),
        "model_provider" => Some(ConfigValue::Str(OFFICIAL_PROVIDER.into())),
        "openai_base_url" => profile.base_url.clone().map(ConfigValue::Str),
        "experimental_bearer_token"
        | CODEX_LEGACY_PROVIDER_NAME_KEY
        | CODEX_LEGACY_PROVIDER_BASE_URL_KEY
        | CODEX_LEGACY_PROVIDER_WIRE_API_KEY
        | CODEX_LEGACY_PROVIDER_TOKEN_KEY => None,
        "model_context_window" => match &profile.model_options {
            Some(ModelOptions::Codex(settings)) => settings
                .context_window
                .map(|tokens| ConfigValue::Number(tokens as f64)),
            None => None,
            Some(ModelOptions::Claude(_)) => {
                unreachable!("profile validation rejects mismatched options")
            }
        },
        _ => unreachable!("Codex provider mapping must be declared in the ownership directory"),
    }
}

/// One common setting's overlay entry: an explicit non-default value is
/// written, while the directory default is expressed by omitting the line.
/// Common-setting intent is explicit: automatic keys leave the host value
/// alone only after removing a previously managed line; explicit values are
/// always written, even when they resemble a documented client default.
fn common_entry(value: &CommonSettingValue) -> OverlayEntry {
    match value {
        CommonSettingValue::Automatic => OverlayEntry::RemoveIfPresent,
        CommonSettingValue::Explicit { value } => OverlayEntry::Set(value.clone()),
    }
}

/// Resolves a stored common preference against the selected provider's
/// protocol capability. Codex server-side web search is a Responses-only
/// service tool; a Chat Completions or Anthropic upstream cannot execute it.
/// The client file therefore receives an explicit disabled value only for the
/// routed configuration, while the stored common preference remains intact
/// and is restored on the next native Responses projection.
fn effective_common_entry(
    profile: &crate::contracts::ProviderProfile,
    key: &str,
    value: &CommonSettingValue,
) -> OverlayEntry {
    let requires_protocol_guard = profile.route_mode == RouteMode::Custom
        && profile
            .upstream_protocol
            .is_some_and(|protocol| protocol != crate::contracts::UpstreamProtocol::Responses);
    if key == CODEX_WEB_SEARCH_KEY && requires_protocol_guard {
        return OverlayEntry::Set(ConfigValue::Str("disabled".to_string()));
    }
    common_entry(value)
}

pub(super) fn common_overlay(common: &CommonSettings) -> Vec<(String, OverlayEntry)> {
    setting_specs(AppKind::Codex)
        .into_iter()
        .filter(|spec| spec.owner == SettingOwner::Common)
        .map(|spec| {
            let value = common
                .value(spec.key)
                .expect("common-settings validation guarantees every catalog key");
            (spec.key.to_string(), common_entry(value))
        })
        .collect()
}

/// Derives all changes by iterating the ownership directory. The only literal
/// names below map declared slots to plan data; no second managed-key list
/// exists in the adapter.
pub(super) fn overlay(plan: &SwitchPlan) -> Vec<(String, OverlayEntry)> {
    let mut entries: Vec<(String, OverlayEntry)> = setting_specs(AppKind::Codex)
        .into_iter()
        .map(|spec| {
            let entry = match spec.owner {
                SettingOwner::Provider => provider_value(&plan.profile, spec.key)
                    .map(OverlayEntry::Set)
                    .unwrap_or_else(|| absent_provider_entry(spec.key)),
                SettingOwner::Common => {
                    let value = plan
                        .common
                        .value(spec.key)
                        .expect("plan validation guarantees complete common settings");
                    effective_common_entry(&plan.profile, spec.key, value)
                }
                SettingOwner::Host => unreachable!("host keys never appear in the directory"),
            };
            (spec.key.to_string(), entry)
        })
        .collect();
    entries.push((
        format!("model_providers.{CODEX_LEGACY_PROVIDER_ID}"),
        OverlayEntry::RemoveTableIfEmpty,
    ));
    entries
}
