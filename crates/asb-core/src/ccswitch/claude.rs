//! Claude-only CC Switch import boundary.

mod auth;
#[cfg(test)]
mod auth_tests;
mod connection;

use super::row::{CcSwitchProposal, CcSwitchProviderDraft, CcSwitchRow};
use super::usage::map_usage_query;
use crate::contracts::{
    AppKind, ProviderDraft, ResponsesOptions, ResponsesRequestMode, RouteMode, SettingsValues,
    UpstreamProtocol,
};
use serde_json::Value;

/// Claude env keys a profile can represent. Either credential alias supplies
/// the profile credential, while its delivery scheme remains independent
/// from the selected body protocol.
const CLAUDE_ENV_KEYS: [&str; 14] = [
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_DEFAULT_FABLE_MODEL",
    "CLAUDE_CODE_SUBAGENT_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
    "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
    "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
    "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME",
    "ANTHROPIC_SMALL_FAST_MODEL",
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
    source_warnings(&config, &parameters, &mut warnings);
    let Some(route) = route else {
        return Ok(CcSwitchProposal {
            key,
            draft: CcSwitchProviderDraft::Claude(official_draft(parameters)),
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
            notes: row.notes.clone(),
            website_url: row.website_url.clone(),
            usage_query,
            official_quota_refresh_interval_minutes: None,
        }),
        warnings,
    })
}

fn official_draft(parameters: SettingsValues) -> ProviderDraft {
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
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn source_warnings(config: &Value, parameters: &SettingsValues, warnings: &mut Vec<String>) {
    let env = config
        .get("env")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    // Names only; values of unrecognized keys never enter the output.
    for name in env.keys() {
        if !CLAUDE_ENV_KEYS.contains(&name.as_str())
            && !crate::claude_native::from_config(config)
                .ok()
                .flatten()
                .is_some_and(|(native, _)| {
                    native.kind.accepts(name)
                        || name == native.kind.flag()
                        || name == native.kind.base_key()
                })
        {
            warnings.push(format!("未导入: env.{name}"));
        }
    }
    for name in config.as_object().map(|o| o.keys()).into_iter().flatten() {
        if ![
            "env",
            "model",
            "availableModels",
            "base_url",
            "baseURL",
            "apiEndpoint",
            "api_format",
            "openrouter_compat_mode",
            "auth_mode",
        ]
        .contains(&name.as_str())
            && !parameters.settings.contains_key(name)
        {
            warnings.push(format!("未导入: {name}"));
        }
    }
}

fn source_env(config: &Value) -> Result<serde_json::Map<String, Value>, String> {
    let env = match config.get("env") {
        None => serde_json::Map::new(),
        Some(Value::Object(env)) => env.clone(),
        Some(_) => return Err("Claude env 必须是 JSON 对象".into()),
    };
    Ok(env)
}
