use crate::contracts::{
    AppKind, ClaudeModelSettings, ImportProposal, ModelOptions, ProviderDraft, RouteMode,
    RouteState, SettingsValues, UpstreamProtocol,
};

use crate::discovery::report::{DiscoveredFile, DiscoveredState};

pub(super) fn codex_auth_api_key(text: Option<&str>) -> Option<String> {
    let root: serde_json::Value = serde_json::from_str(text?).ok()?;
    (root.get("auth_mode").and_then(serde_json::Value::as_str) == Some("apikey"))
        .then(|| {
            root.get("OPENAI_API_KEY")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .flatten()
        .filter(|value| !value.trim().is_empty())
}

/// Converts only the externally valid Claude wire spelling into the profile
/// contract. The resulting profile never carries a `[1m]` suffix in a model
/// string; enabled context lives in the explicit boolean fields.
pub(super) fn claude_import_model_fields(
    route: &RouteState,
) -> Result<(Option<String>, Option<ModelOptions>), String> {
    let (model, primary_one_m) =
        crate::claude_model::parse_optional_model(route.model.as_deref(), "主模型", true)?;
    let (haiku_model, _) =
        crate::claude_model::parse_optional_model(route.haiku_model.as_deref(), "Haiku 档", false)?;
    let (sonnet_model, sonnet_one_m) = crate::claude_model::parse_optional_model(
        route.sonnet_model.as_deref(),
        "Sonnet 档",
        true,
    )?;
    let (opus_model, opus_one_m) =
        crate::claude_model::parse_optional_model(route.opus_model.as_deref(), "Opus 档", true)?;
    let available_models = match route.available_models.as_ref() {
        Some(models) => {
            for model in models {
                crate::claude_model::parse_model(model, "可选模型列表", false)?;
            }
            Some(models.clone())
        }
        None => None,
    };
    let has_settings = primary_one_m
        || haiku_model.is_some()
        || sonnet_model.is_some()
        || sonnet_one_m
        || opus_model.is_some()
        || opus_one_m
        || available_models.is_some();
    let model_options = has_settings.then(|| {
        ModelOptions::Claude(ClaudeModelSettings {
            primary_one_m,
            haiku_model,
            sonnet_model,
            sonnet_one_m,
            opus_model,
            opus_one_m,
            available_models,
        })
    });
    Ok((model, model_options))
}

/// Builds an import proposal from a discovered file and its locally-read raw
/// configuration. The API key enters only the returned draft; route state,
/// warnings and diagnostics never carry its value.
pub fn import_proposal(
    file: &DiscoveredFile,
    text: Option<&str>,
    codex_auth: Option<&str>,
) -> Option<ImportProposal> {
    let text = text?;
    let DiscoveredState::Ok {
        route,
        importable: true,
        ..
    } = &file.state
    else {
        return None;
    };
    let parameters = crate::adapter::read_provider_parameters(file.app, text).ok()?;
    if route.route_mode == RouteMode::Official {
        return Some(official_proposal(file.app, parameters));
    }
    match file.app {
        AppKind::Codex => codex_proposal(route, parameters, text, codex_auth),
        AppKind::Claude => claude_proposal(route, parameters, text),
    }
}

fn official_proposal(app: AppKind, parameters: SettingsValues) -> ImportProposal {
    let name = match app {
        AppKind::Codex => "Codex 官方登录",
        AppKind::Claude => "Claude 官方登录",
    };
    ImportProposal {
        app,
        draft: ProviderDraft {
            app,
            route_mode: RouteMode::Official,
            name: name.to_string(),
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
        },
        basis: format!("由当前 {name} 状态生成；凭据继续由客户端管理"),
    }
}

fn codex_proposal(
    route: &RouteState,
    parameters: SettingsValues,
    text: &str,
    codex_auth: Option<&str>,
) -> Option<ImportProposal> {
    let key = codex_auth_api_key(codex_auth)?;
    let (upstream_protocol, responses_options) = super::codex::import_route(text, route).ok()?;
    let model_options = route
        .codex_model_options
        .clone()
        .filter(|options| options.context_window.is_some())
        .map(ModelOptions::Codex);
    Some(ImportProposal {
        app: AppKind::Codex,
        draft: ProviderDraft {
            app: AppKind::Codex,
            route_mode: RouteMode::Custom,
            name: route
                .provider_name
                .clone()
                .unwrap_or_else(|| "当前 Codex 配置".to_string()),
            model: route.model.clone(),
            base_url: route.base_url.clone(),
            api_key: key,
            upstream_protocol: Some(upstream_protocol),
            responses_options,
            max_output_tokens: None.into(),
            model_options,
            parameters,
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        },
        basis: "由当前 Codex 可转换配置生成".to_string(),
    })
}

fn claude_proposal(
    route: &RouteState,
    parameters: SettingsValues,
    text: &str,
) -> Option<ImportProposal> {
    let root: serde_json::Value = serde_json::from_str(text).ok()?;
    let key = root
        .pointer("/env/ANTHROPIC_AUTH_TOKEN")
        .or_else(|| root.pointer("/env/ANTHROPIC_API_KEY"))
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())?;
    let (model, model_options) = claude_import_model_fields(route).ok()?;
    Some(ImportProposal {
        app: AppKind::Claude,
        draft: ProviderDraft {
            app: AppKind::Claude,
            route_mode: RouteMode::Custom,
            name: "当前 Claude 配置".to_string(),
            model,
            base_url: route.base_url.clone(),
            api_key: key.to_string(),
            upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
            responses_options: None,
            max_output_tokens: None.into(),
            model_options,
            parameters,
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        },
        basis: "由当前 Claude 配置的模型与服务地址生成".to_string(),
    })
}
