use crate::contracts::{
    AppKind, AuthenticationScheme, ProviderDraft, RouteMode, RouteState, SettingsValues,
    UpstreamProtocol,
};

use crate::discovery::report::{ClaudeImportProposal, DiscoveredFile, DiscoveredState};

/// Builds a Claude import proposal from its discovered locally-read raw
/// configuration. Codex has its own complete profile contract and is never
/// representable as a generic import proposal.
pub fn claude_import_proposal(
    file: &DiscoveredFile,
    text: Option<&str>,
) -> Option<ClaudeImportProposal> {
    let text = text?;
    let DiscoveredState::Ok {
        route,
        importable: true,
        ..
    } = &file.state
    else {
        return None;
    };
    match file.app {
        AppKind::Codex => None,
        AppKind::Claude => {
            let parameters =
                crate::adapter::read_provider_parameters(AppKind::Claude, text).ok()?;
            if route.route_mode == RouteMode::Official {
                Some(official_proposal(parameters))
            } else {
                claude_proposal(route, parameters, text)
            }
        }
    }
}

fn official_proposal(parameters: SettingsValues) -> ClaudeImportProposal {
    let name = "Claude 官方登录";
    ClaudeImportProposal {
        draft: ProviderDraft {
            authentication: None,
            app: AppKind::Claude,
            route_mode: RouteMode::Official,
            name: name.to_string(),
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
        },
        basis: format!("由当前 {name} 状态生成；凭据继续由客户端管理"),
    }
}

fn claude_proposal(
    route: &RouteState,
    parameters: SettingsValues,
    text: &str,
) -> Option<ClaudeImportProposal> {
    let root: serde_json::Value = serde_json::from_str(text).ok()?;
    if let Some((native, base_url)) = crate::claude_native::from_config(&root).ok()? {
        let (model, model_options) = crate::claude_model::import_models(&root, crate::claude_model::ModelSource::Client).ok()?;
        let mut proposal = official_proposal(parameters);
        proposal.draft.route_mode = RouteMode::Custom; proposal.draft.name = "当前 Claude 原生云配置".into();
        proposal.draft.upstream_protocol = Some(UpstreamProtocol::AnthropicMessages);
        proposal.draft.connection.claude_native = Some(native); proposal.draft.base_url = base_url;
        proposal.draft.model = model; proposal.draft.model_options = model_options;
        proposal.basis = "由当前 Claude 原生云 SDK 配置生成，不读取云凭据缓存".into();
        return Some(proposal);
    }
    let credential = |path| {
        root.pointer(path)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
    };
    let (authentication, key) = credential("/env/ANTHROPIC_AUTH_TOKEN")
        .map(|key| (AuthenticationScheme::Bearer, key))
        .or_else(|| {
            credential("/env/ANTHROPIC_API_KEY").map(|key| (AuthenticationScheme::XApiKey, key))
        })?;
    let (model, model_options) =
        crate::claude_model::import_models(&root, crate::claude_model::ModelSource::Client).ok()?;
    Some(ClaudeImportProposal {
        draft: ProviderDraft {
            authentication: Some(authentication),
            app: AppKind::Claude,
            route_mode: RouteMode::Custom,
            name: "当前 Claude 配置".to_string(),
            model,
            base_url: route.base_url.clone(),
            connection: Default::default(),
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
