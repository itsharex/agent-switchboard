//! Claude-only the source application import boundary.

mod auth;
#[cfg(test)]
mod auth_tests;
mod connection;

use super::row::{CcSwitchProposal, CcSwitchProviderDraft, CcSwitchRow};
use super::usage::map_usage_query;
use crate::claude_common::Extra;
use crate::contracts::{
    AppKind, ProviderDraft, ResponsesOptions, ResponsesRequestMode, RouteMode, SettingsValues,
    UpstreamProtocol,
};
use serde_json::Value;

/// Top-level source keys consumed by the the source application structured mapping; they
/// never reach the profile fragment.
const RESERVED_TOP_KEYS: [&str; 6] = [
    "base_url",
    "baseURL",
    "apiEndpoint",
    "api_format",
    "openrouter_compat_mode",
    "auth_mode",
];

/// ChatGPT Codex catalogs gpt-5.6 at a 372K context window, far below the
/// 1.05M API spec, so the routed Claude Code default of 200K wastes context.
/// Kimi For Coding serves a 256K window under the same 200K default.
const CODEX_OAUTH_CONTEXT_TOKENS: &str = "372000";
const KIMI_FOR_CODING_CONTEXT_TOKENS: &str = "262144";
const KIMI_FOR_CODING_BASE_URL: &str = "https://api.kimi.com/coding";
const CONTEXT_ENV_KEYS: [&str; 2] = [
    "CLAUDE_CODE_MAX_CONTEXT_TOKENS",
    "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
];
const CODEX_OAUTH_MODEL_ENV_KEYS: [&str; 6] = [
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
];

/// Converts explicit source metadata at the import boundary. No source
/// alias survives in the application profile store.
pub(super) fn map_claude(key: String, row: &CcSwitchRow) -> Result<CcSwitchProposal, String> {
    let config: Value =
        serde_json::from_str(&row.settings_config).map_err(|e| format!("配置无法解析: {e}"))?;
    let parameters =
        crate::adapter::read_provider_parameters(AppKind::Claude, &row.settings_config)
            .map_err(|error| error.to_string())?;
    let env = source_env(&config)?;
    let mut warnings = Vec::new();
    let meta = connection::meta(row.meta.as_deref(), &mut warnings)?;
    let route = connection::route(&config, &env, &meta, &mut warnings)?;
    let (model, model_options) =
        crate::claude_model::import_models(&config, crate::claude_model::ModelSource::CcSwitch)?;
    let mut fragment = source_fragment(&config, &parameters, &mut warnings);
    if let Some(route) = route.as_ref() {
        context_defaults(route, &env, &mut fragment);
    }
    let Some(route) = route else {
        return Ok(CcSwitchProposal {
            key,
            draft: CcSwitchProviderDraft::Claude(official_draft(
                parameters,
                fragment,
                row.display.clone(),
            )),
            warnings,
        });
    };
    let usage_query = map_usage_query(row.meta.as_deref(), &mut warnings);
    Ok(CcSwitchProposal {
        key,
        draft: CcSwitchProviderDraft::Claude(ProviderDraft {
            authentication: route
                .connection
                .claude_native
                .is_none()
                .then_some(route.authentication),
            app: AppKind::Claude,
            route_mode: RouteMode::Custom,
            name: row.name.clone(),
            model,
            base_url: (!route.base_url.is_empty()).then_some(route.base_url),
            connection: route.connection,
            api_key: route.api_key,
            upstream_protocol: Some(route.protocol),
            responses_options: (route.protocol == UpstreamProtocol::Responses).then_some(
                ResponsesOptions {
                    request_mode: ResponsesRequestMode::Standard,
                },
            ),
            max_output_tokens: None.into(),
            model_options,
            parameters,
            claude_fragment: fragment,
            notes: row.notes.clone(),
            website_url: row.website_url.clone(),
            display: row.display.clone(),
            usage_query,
            official_quota_refresh_interval_minutes: None,
        }),
        warnings,
    })
}

fn official_draft(
    parameters: SettingsValues,
    fragment: Extra,
    display: Option<crate::contracts::ProviderDisplay>,
) -> ProviderDraft {
    ProviderDraft {
        authentication: None,
        app: AppKind::Claude,
        route_mode: RouteMode::Official,
        name: "Claude 官方登录".to_string(),
        model: None,
        base_url: None,
        connection: Default::default(),
        api_key: String::new(),
        upstream_protocol: None,
        responses_options: None,
        max_output_tokens: None.into(),
        model_options: None,
        parameters,
        claude_fragment: fragment,
        notes: None,
        website_url: None,
        display,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

/// Collects source settings the structured profile cannot express into the
/// profile-owned fragment. Keys rejected by fragment ownership stay in the
/// warnings exactly as before; keys the fragment accepts stop being losses.
fn source_fragment(
    config: &Value,
    parameters: &SettingsValues,
    warnings: &mut Vec<String>,
) -> Extra {
    let (kept, dropped) =
        crate::claude_common::import_fragment(config, parameters, &RESERVED_TOP_KEYS);
    for name in dropped {
        warnings.push(format!("未导入: {name}"));
    }
    kept
}

/// Route-calibrated context-window defaults, applied at the import boundary
/// so they stay visible, profile-owned values instead of hidden render-time
/// injections. Explicit source values always win.
fn context_defaults(
    route: &connection::SourceRoute,
    env: &serde_json::Map<String, Value>,
    fragment: &mut Extra,
) {
    let default =
        if route.connection.provider_type.as_deref() == Some("codex_oauth") && targets_gpt56(env) {
            Some(CODEX_OAUTH_CONTEXT_TOKENS)
        } else if route.base_url.trim().trim_end_matches('/') == KIMI_FOR_CODING_BASE_URL {
            Some(KIMI_FOR_CODING_CONTEXT_TOKENS)
        } else {
            None
        };
    let Some(default) = default else {
        return;
    };
    let entry = fragment
        .entry("env".to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(env) = entry.as_object_mut() {
        for key in CONTEXT_ENV_KEYS {
            env.entry(key.to_string())
                .or_insert_with(|| Value::String(default.to_string()));
        }
    }
}

/// The gpt-5.6 family is the only Codex-OAuth model set calibrated for the
/// injected window; any other model family keeps its own catalog defaults.
fn targets_gpt56(env: &serde_json::Map<String, Value>) -> bool {
    let mut saw_model = false;
    for key in CODEX_OAUTH_MODEL_ENV_KEYS {
        let Some(Value::String(model)) = env.get(key) else {
            continue;
        };
        let model = model.trim();
        if model.is_empty() {
            continue;
        }
        saw_model = true;
        if !model.to_ascii_lowercase().starts_with("gpt-5.6") {
            return false;
        }
    }
    saw_model
}

fn source_env(config: &Value) -> Result<serde_json::Map<String, Value>, String> {
    let env = match config.get("env") {
        None => serde_json::Map::new(),
        Some(Value::Object(env)) => env.clone(),
        Some(_) => return Err("Claude env 必须是 JSON 对象".into()),
    };
    Ok(env)
}
