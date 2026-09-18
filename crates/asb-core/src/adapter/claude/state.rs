use serde_json::Value as Json;

use crate::adapter::AdapterError;
use crate::contracts::{
    AppKind, AuthenticationScheme, KeyChange, RouteMode, RouteState, SwitchPlan,
};
use crate::ownership::is_owned;

use crate::adapter::claude::document::{get, parse, scalar_repr};
use crate::adapter::claude::overlay::ENV_MODEL_KEY;

pub(crate) fn matches_provider_credentials(
    current: &str,
    plan: &SwitchPlan,
) -> Result<bool, AdapterError> {
    let profile = &plan.profile;
    if !super::native::matches(current, plan)? {
        return Ok(false);
    }
    if profile.connection.claude_native.is_some() {
        return Ok(true);
    }
    let root = parse(current)?;
    let token = get(&root, "env.ANTHROPIC_AUTH_TOKEN").and_then(Json::as_str);
    let api_key = get(&root, "env.ANTHROPIC_API_KEY").and_then(Json::as_str);
    Ok(match profile.route_mode {
        RouteMode::Official => token.is_none_or(str::is_empty) && api_key.is_none_or(str::is_empty),
        RouteMode::Custom => match plan.client_authentication() {
            Some(AuthenticationScheme::Bearer) => {
                !plan.client_api_key().is_empty()
                    && token == Some(plan.client_api_key())
                    && api_key.is_none_or(str::is_empty)
            }
            Some(AuthenticationScheme::XApiKey) => {
                !plan.client_api_key().is_empty()
                    && api_key == Some(plan.client_api_key())
                    && token.is_none_or(str::is_empty)
            }
            Some(AuthenticationScheme::XGoogApiKey) | None => false,
        },
    })
}

/// Collects every scalar or array path accepted by `keep` with its textual value.
fn collect_scalars(
    value: &Json,
    prefix: &str,
    out: &mut std::collections::BTreeMap<String, String>,
    keep: &dyn Fn(&str) -> bool,
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
            if keep(&path) {
                out.insert(path.clone(), repr);
            }
        }
        collect_scalars(child, &path, out, keep);
    }
}

fn collect_with(
    value: &Json,
    keep: &dyn Fn(&str) -> bool,
) -> std::collections::BTreeMap<String, String> {
    let mut out = std::collections::BTreeMap::new();
    collect_scalars(value, "", &mut out, keep);
    out
}

/// Owned-key diff between the live text and a previous copy.
pub(crate) fn owned_diff(current: &str, previous: &str) -> Result<Vec<KeyChange>, AdapterError> {
    let mut native = super::native::owned_paths(&parse(current)?)?;
    native.extend(super::native::owned_paths(&parse(previous)?)?);
    let keep = |path: &str| is_owned(AppKind::Claude, path) || native.contains(path);
    let mut changes = crate::adapter::diff_owned_maps(
        &collect_with(&parse(current)?, &keep),
        &collect_with(&parse(previous)?, &keep),
    );
    changes.extend(
        crate::claude_common::diff_documents(current, previous).map_err(|message| {
            AdapterError {
                message,
                line: None,
            }
        })?,
    );
    Ok(changes)
}

/// Every-leaf diff, including host-owned keys: the deep reset preview must
/// list unmanaged removals, which [`owned_diff`] deliberately hides. The
/// all-leaf walk already covers manifest-claimed paths, so no separate
/// claude-common pass is needed.
pub(crate) fn full_diff(current: &str, previous: &str) -> Result<Vec<KeyChange>, AdapterError> {
    Ok(crate::adapter::diff_owned_maps(
        &collect_with(&parse(current)?, &|_| true),
        &collect_with(&parse(previous)?, &|_| true),
    ))
}

/// Reads the active routing facts from Claude settings text.
pub fn route_state(text: &str) -> RouteState {
    let root = parse(text).expect("caller validates syntax first");
    let string_at = |path: &str| get(&root, path).and_then(|v| v.as_str().map(str::to_string));
    let native = crate::claude_native::from_config(&root).ok().flatten();
    let base_url = native
        .as_ref()
        .and_then(|(_, base)| base.clone())
        .or_else(|| string_at("env.ANTHROPIC_BASE_URL"));
    // env.ANTHROPIC_MODEL overrides the top-level `model` when present, so it
    // is the model that actually takes effect.
    let model = string_at(ENV_MODEL_KEY).or_else(|| string_at("model"));
    let haiku_model = string_at("env.ANTHROPIC_DEFAULT_HAIKU_MODEL");
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
        route_mode: if native.is_some() || base_url.is_some() {
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
