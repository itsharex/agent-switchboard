use serde_json::Value as Json;

use crate::adapter::AdapterError;
use crate::contracts::{
    AppKind, AuthenticationScheme, KeyChange, RouteMode, RouteState, SwitchPlan,
};
use crate::ownership::is_owned;

use crate::adapter::claude::document::{get, parse, scalar_repr};
use crate::adapter::claude::overlay::{DEPRECATED_MODEL_KEY, ENV_MODEL_KEY};

pub(crate) fn matches_provider_credentials(
    current: &str,
    plan: &SwitchPlan,
) -> Result<bool, AdapterError> {
    let profile = &plan.profile;
    let root = parse(current)?;
    let token = get(&root, "env.ANTHROPIC_AUTH_TOKEN").and_then(Json::as_str);
    let api_key = get(&root, "env.ANTHROPIC_API_KEY").and_then(Json::as_str);
    Ok(match profile.route_mode {
        RouteMode::Official => token.is_none_or(str::is_empty) && api_key.is_none_or(str::is_empty),
        RouteMode::Custom => match plan.client_authentication() {
            Some(AuthenticationScheme::Bearer) => {
                !profile.api_key.is_empty()
                    && token == Some(profile.api_key.as_str())
                    && api_key.is_none_or(str::is_empty)
            }
            Some(AuthenticationScheme::XApiKey) => {
                !profile.api_key.is_empty()
                    && api_key == Some(profile.api_key.as_str())
                    && token.is_none_or(str::is_empty)
            }
            None => false,
        },
    })
}

/// Collects every owned scalar or array path and its textual value.
fn collect_owned_scalars(
    value: &Json,
    prefix: &str,
    out: &mut std::collections::BTreeMap<String, String>,
) {
    let Json::Object(map) = value else {
        return;
    };
    for (key, child) in map {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        if let Some(repr) = scalar_repr(child) {
            if is_owned(AppKind::Claude, &path) {
                out.insert(path.clone(), repr);
            }
        }
        collect_owned_scalars(child, &path, out);
    }
}

/// Owned-key diff between the live text and a previous copy.
pub(crate) fn owned_diff(current: &str, previous: &str) -> Result<Vec<KeyChange>, AdapterError> {
    let mut current_values = std::collections::BTreeMap::new();
    collect_owned_scalars(&parse(current)?, "", &mut current_values);
    let mut previous_values = std::collections::BTreeMap::new();
    collect_owned_scalars(&parse(previous)?, "", &mut previous_values);
    Ok(crate::adapter::diff_owned_maps(
        &current_values,
        &previous_values,
    ))
}

/// Reads the active routing facts from Claude settings text.
pub fn route_state(text: &str) -> RouteState {
    let root = parse(text).expect("caller validates syntax first");
    let string_at = |path: &str| get(&root, path).and_then(|v| v.as_str().map(str::to_string));
    let base_url = string_at("env.ANTHROPIC_BASE_URL");
    // env.ANTHROPIC_MODEL overrides the top-level `model` when present, so it
    // is the model that actually takes effect.
    let model = string_at(ENV_MODEL_KEY).or_else(|| string_at("model"));
    // The Haiku tier falls back to the deprecated key so old files import
    // cleanly; the adapter removes the deprecated key on the next switch.
    let haiku_model =
        string_at("env.ANTHROPIC_DEFAULT_HAIKU_MODEL").or_else(|| string_at(DEPRECATED_MODEL_KEY));
    let available_models = get(&root, "availableModels").and_then(|v| {
        v.as_array().map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect::<Vec<String>>()
        })
    });
    RouteState {
        app: AppKind::Claude,
        route_mode: if base_url.is_some() {
            RouteMode::Custom
        } else {
            RouteMode::Official
        },
        provider_name: None,
        model,
        base_url,
        wire_api: None,
        codex_model_options: None,
        haiku_model,
        sonnet_model: string_at("env.ANTHROPIC_DEFAULT_SONNET_MODEL"),
        opus_model: string_at("env.ANTHROPIC_DEFAULT_OPUS_MODEL"),
        available_models,
        scope_warnings: vec![],
    }
}
