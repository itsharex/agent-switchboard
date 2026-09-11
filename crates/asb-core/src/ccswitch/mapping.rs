use serde_json::Value;

use crate::contracts::{
    AppKind, ClaudeModelSettings, ModelOptions, ProviderDraft, ResponsesOptions,
    ResponsesRequestMode, RouteMode, SettingsValues, UpstreamProtocol,
};

use crate::ccswitch::row::{CcSwitchProposal, CcSwitchProviderDraft, CcSwitchRow, CcSwitchSkip};
use crate::ccswitch::usage::map_usage_query;

/// Claude env keys a profile can represent. Either credential alias supplies
/// the profile API key; the configured `ANTHROPIC_AUTH_TOKEN` is rendered on
/// activation.
const CLAUDE_ENV_KEYS: [&str; 7] = [
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_API_KEY",
];

/// Converts explicit source metadata at the import boundary. No source
/// alias survives in the application profile store.
fn source_protocol(
    meta: Option<&str>,
    fallback: UpstreamProtocol,
) -> Result<UpstreamProtocol, String> {
    let Some(meta) = meta.filter(|value| !value.trim().is_empty()) else {
        return Ok(fallback);
    };
    let root: Value = serde_json::from_str(meta).map_err(|_| "meta 无法解析".to_string())?;
    let Some(value) = root
        .get("apiFormat")
        .or_else(|| root.get("api_format"))
        .and_then(Value::as_str)
    else {
        return Ok(fallback);
    };
    match value {
        "openai_responses" | "responses" => Ok(UpstreamProtocol::Responses),
        "openai_chat" | "chat" | "chat_completions" => Ok(UpstreamProtocol::ChatCompletions),
        "anthropic" | "anthropic_messages" => Ok(UpstreamProtocol::AnthropicMessages),
        _ => Err(format!("meta.apiFormat = {value} 不受支持")),
    }
}

/// Maps one source row into a proposal or a skip.
pub fn map_row(row: &CcSwitchRow) -> Result<CcSwitchProposal, CcSwitchSkip> {
    let key = format!("{}:{}", row.app_type, row.id);
    match row.app_type.as_str() {
        "codex" => crate::ccswitch::codex::map_codex(key.clone(), row)
            .map_err(|reason| skip_with(key, row, reason)),
        "claude" => map_claude(key.clone(), row).map_err(|reason| skip_with(key, row, reason)),
        other => Err(skip_with(
            key,
            row,
            format!("客户端 {other} 超出本应用支持范围"),
        )),
    }
}

fn skip_with(key: String, row: &CcSwitchRow, reason: String) -> CcSwitchSkip {
    CcSwitchSkip {
        key,
        app_type: row.app_type.clone(),
        name: row.name.clone(),
        reason,
    }
}

fn map_claude(key: String, row: &CcSwitchRow) -> Result<CcSwitchProposal, String> {
    let config: Value =
        serde_json::from_str(&row.settings_config).map_err(|e| format!("配置无法解析: {e}"))?;
    let parameters =
        crate::adapter::read_provider_parameters(AppKind::Claude, &row.settings_config)
            .map_err(|error| error.to_string())?;
    let env = config.get("env").cloned().unwrap_or(Value::Null);
    let env = env.as_object().cloned().unwrap_or_default();

    let text = |name: &str| {
        env.get(name)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
    };
    let base_url = text("ANTHROPIC_BASE_URL");
    let (model, model_options) = claude_models(&env)?;

    // Names only; values of unrecognized keys never enter the output.
    let mut warnings = Vec::new();
    for name in env.keys() {
        if !CLAUDE_ENV_KEYS.contains(&name.as_str()) {
            warnings.push(format!("未导入: env.{name}"));
        }
    }
    for name in config.as_object().map(|o| o.keys()).into_iter().flatten() {
        if name != "env" && !parameters.settings.contains_key(name) {
            warnings.push(format!("未导入: {name}"));
        }
    }

    // A settings.json without a custom endpoint is an official route. Its
    // credentials stay client-owned; only the selectable route is imported.
    let base_url = match base_url {
        Some(url) => url,
        None => {
            return Ok(CcSwitchProposal {
                key,
                draft: CcSwitchProviderDraft::Claude(official_draft(parameters)),
                warnings,
            });
        }
    };
    let api_key = text("ANTHROPIC_AUTH_TOKEN")
        .or_else(|| text("ANTHROPIC_API_KEY"))
        .ok_or_else(|| "缺少 ANTHROPIC_AUTH_TOKEN 或 ANTHROPIC_API_KEY".to_string())?;
    let upstream_protocol =
        source_protocol(row.meta.as_deref(), UpstreamProtocol::AnthropicMessages)?;
    let usage_query = map_usage_query(row.meta.as_deref(), &mut warnings);
    Ok(CcSwitchProposal {
        key,
        draft: CcSwitchProviderDraft::Claude(ProviderDraft {
            app: AppKind::Claude,
            route_mode: RouteMode::Custom,
            name: row.name.clone(),
            model,
            base_url: Some(base_url.to_string()),
            api_key: api_key.to_string(),
            upstream_protocol: Some(upstream_protocol),
            responses_options: (upstream_protocol == UpstreamProtocol::Responses).then_some(
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

fn claude_models(
    env: &serde_json::Map<String, Value>,
) -> Result<(Option<String>, Option<ModelOptions>), String> {
    let text = |name: &str| {
        env.get(name)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
    };
    let (model, primary_one_m) =
        crate::claude_model::parse_ccswitch_model(text("ANTHROPIC_MODEL"), "主模型", true)?;
    let (haiku, _) = crate::claude_model::parse_ccswitch_model(
        text("ANTHROPIC_DEFAULT_HAIKU_MODEL"),
        "Haiku 档",
        false,
    )?;
    let (sonnet, sonnet_one_m) = crate::claude_model::parse_ccswitch_model(
        text("ANTHROPIC_DEFAULT_SONNET_MODEL"),
        "Sonnet 档",
        true,
    )?;
    let (opus, opus_one_m) = crate::claude_model::parse_ccswitch_model(
        text("ANTHROPIC_DEFAULT_OPUS_MODEL"),
        "Opus 档",
        true,
    )?;

    let model_options = (primary_one_m
        || haiku.is_some()
        || sonnet.is_some()
        || sonnet_one_m
        || opus.is_some()
        || opus_one_m)
        .then(|| {
            ModelOptions::Claude(ClaudeModelSettings {
                primary_one_m,
                haiku_model: haiku,
                sonnet_model: sonnet,
                sonnet_one_m,
                opus_model: opus,
                opus_one_m,
                available_models: None,
            })
        });
    Ok((model, model_options))
}

fn official_draft(parameters: SettingsValues) -> ProviderDraft {
    ProviderDraft {
        app: AppKind::Claude,
        route_mode: RouteMode::Official,
        name: "Claude 官方登录".to_string(),
        model: None,
        base_url: None,
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
