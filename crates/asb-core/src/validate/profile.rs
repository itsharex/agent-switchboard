use crate::contracts::{
    AppKind, AuthenticationScheme, ClaudeModelSettings, CodexModelSettings, ModelOptions,
    ProviderConnectionOptions, ProviderDraft, ProviderProfile, ResponsesOptions, RouteMode,
    UpstreamProtocol, UsageQuery,
};

use crate::validate::error::{ValidationError, MAX_AUTO_REFRESH_INTERVAL_MINUTES, MAX_NOTES_LEN};
use crate::validate::validate_usage_query;

impl ProviderProfile {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.id.trim().is_empty() {
            return Err(ValidationError::EmptyId);
        }
        self.parameters.validate_provider_parameters(self.app)?;
        validate_profile_fields(ProfileFields {
            app: self.app,
            route_mode: self.route_mode,
            name: &self.name,
            model: self.model.as_deref(),
            base_url: self.base_url.as_deref(),
            connection: &self.connection,
            api_key: &self.api_key,
            authentication: self.authentication,
            upstream_protocol: self.upstream_protocol,
            responses_options: self.responses_options,
            max_output_tokens: self.max_output_tokens,
            model_options: self.model_options.as_ref(),
            notes: self.notes.as_deref(),
            website_url: self.website_url.as_deref(),
            usage_query: self.usage_query.as_ref(),
            official_quota_refresh_interval: self.official_quota_refresh_interval_minutes,
        })
    }
}

impl ProviderDraft {
    pub fn validate(&self) -> Result<(), ValidationError> {
        self.parameters.validate_provider_parameters(self.app)?;
        validate_profile_fields(ProfileFields {
            app: self.app,
            route_mode: self.route_mode,
            name: &self.name,
            model: self.model.as_deref(),
            base_url: self.base_url.as_deref(),
            connection: &self.connection,
            api_key: &self.api_key,
            authentication: self.authentication,
            upstream_protocol: self.upstream_protocol,
            responses_options: self.responses_options,
            max_output_tokens: self.max_output_tokens,
            model_options: self.model_options.as_ref(),
            notes: self.notes.as_deref(),
            website_url: self.website_url.as_deref(),
            usage_query: self.usage_query.as_ref(),
            official_quota_refresh_interval: self.official_quota_refresh_interval_minutes,
        })
    }
}

struct ProfileFields<'a> {
    app: AppKind,
    route_mode: RouteMode,
    name: &'a str,
    model: Option<&'a str>,
    base_url: Option<&'a str>,
    connection: &'a ProviderConnectionOptions,
    api_key: &'a str,
    authentication: Option<AuthenticationScheme>,
    upstream_protocol: Option<UpstreamProtocol>,
    responses_options: Option<ResponsesOptions>,
    max_output_tokens: crate::contracts::ExplicitMaxOutputTokens,
    model_options: Option<&'a ModelOptions>,
    notes: Option<&'a str>,
    website_url: Option<&'a str>,
    usage_query: Option<&'a UsageQuery>,
    official_quota_refresh_interval: Option<u32>,
}

fn validate_profile_fields(fields: ProfileFields<'_>) -> Result<(), ValidationError> {
    if fields.name.trim().is_empty() {
        return Err(ValidationError::EmptyName);
    }
    validate_route_fields(&fields)?;
    let ProfileFields {
        app,
        model,
        model_options,
        notes,
        website_url,
        usage_query,
        official_quota_refresh_interval,
        ..
    } = fields;
    validate_model_identifier(model, "主模型")?;
    if let Some(options) = model_options {
        validate_model_options(app, options, model)?;
    }
    if let Some(notes) = notes {
        if notes.chars().count() > MAX_NOTES_LEN {
            return Err(ValidationError::NotesTooLong(MAX_NOTES_LEN));
        }
    }
    if let Some(url) = website_url {
        let ok = url.starts_with("https://") || url.starts_with("http://");
        if !ok {
            return Err(ValidationError::BadWebsiteUrl(url.to_string()));
        }
    }
    if let Some(query) = usage_query {
        validate_usage_query(query)?;
    }
    if let Some(minutes) = official_quota_refresh_interval {
        if minutes == 0 || minutes > MAX_AUTO_REFRESH_INTERVAL_MINUTES {
            return Err(ValidationError::OfficialQuotaRefreshIntervalOutOfRange(
                MAX_AUTO_REFRESH_INTERVAL_MINUTES,
            ));
        }
    }
    Ok(())
}

fn validate_route_fields(fields: &ProfileFields<'_>) -> Result<(), ValidationError> {
    match fields.route_mode {
        RouteMode::Official => {
            if fields.base_url.is_some()
                || !fields.connection.is_empty()
                || !fields.api_key.trim().is_empty()
                || fields.authentication.is_some()
                || fields.upstream_protocol.is_some()
                || fields.responses_options.is_some()
                || fields.max_output_tokens.is_some()
                || fields.model.is_some()
                || fields.model_options.is_some()
                || fields.usage_query.is_some()
            {
                return Err(ValidationError::OfficialRouteHasCustomFields);
            }
            if fields.official_quota_refresh_interval.is_some() && fields.app != AppKind::Codex {
                return Err(ValidationError::QuotaIntervalRequiresOfficialCodex);
            }
        }
        RouteMode::Custom => validate_custom_route(fields)?,
    }
    Ok(())
}

fn validate_custom_route(fields: &ProfileFields<'_>) -> Result<(), ValidationError> {
    if fields.app != AppKind::Codex && fields.connection.codex.is_some() {
        return Err(ValidationError::CodexOptionsRequireCodex);
    }
    if fields.official_quota_refresh_interval.is_some() {
        return Err(ValidationError::QuotaIntervalRequiresOfficialCodex);
    }
    if let Some(native) = &fields.connection.claude_native {
        return super::claude::validate_native(
            fields.app,
            native,
            fields.base_url,
            fields.connection,
            fields.api_key,
            fields.authentication,
            fields.upstream_protocol,
            fields.responses_options.is_some() || fields.max_output_tokens.is_some(),
        );
    }
    let url = fields
        .base_url
        .ok_or(ValidationError::CustomRequiresBaseUrl)?;
    let protocol = fields
        .upstream_protocol
        .ok_or(ValidationError::CustomRequiresProtocol)?;
    validate_connection(url, protocol, fields.connection)?;
    let managed_auth = match fields.app {
        AppKind::Codex => {
            super::codex::validate_connection(fields.connection, protocol, fields.authentication)?
        }
        AppKind::Claude => super::claude::validate_connection(
            fields.app,
            fields.connection,
            protocol,
            fields.authentication,
            fields.api_key,
        )?,
    };
    if !managed_auth && fields.api_key.trim().is_empty() {
        return Err(ValidationError::EmptyApiKey);
    }
    if fields.api_key.chars().any(char::is_control) {
        return Err(ValidationError::InvalidApiKeyCharacters);
    }
    validate_responses_options(protocol, fields.responses_options)?;
    let requires_max_output_tokens =
        fields.app == AppKind::Codex && protocol == UpstreamProtocol::AnthropicMessages;
    if requires_max_output_tokens {
        if !fields
            .max_output_tokens
            .value()
            .is_some_and(|value| value > 0)
        {
            return Err(ValidationError::CodexAnthropicRequiresMaxOutputTokens);
        }
    } else if fields.max_output_tokens.is_some() {
        return Err(ValidationError::UnexpectedMaxOutputTokens);
    }
    if fields.api_key.chars().count() > 4_096 {
        return Err(ValidationError::ApiKeyTooLong(4_096));
    }
    Ok(())
}

fn validate_connection(
    base_url: &str,
    protocol: UpstreamProtocol,
    connection: &ProviderConnectionOptions,
) -> Result<(), ValidationError> {
    let validate_url = |url: &str| {
        if connection.is_full_url {
            crate::endpoint::validate_full_url(url).map_err(ValidationError::BadFullUrl)
        } else {
            crate::endpoint::validate_base_url(url, protocol).map_err(ValidationError::BadBaseUrl)
        }
    };
    validate_url(base_url)?;
    for (key, endpoint) in &connection.custom_endpoints {
        validate_url(key)?;
        validate_url(&endpoint.url)?;
    }
    if let Some(user_agent) = connection.custom_user_agent.as_deref() {
        if user_agent.trim().is_empty() || user_agent.chars().any(char::is_control) {
            return Err(ValidationError::InvalidCustomUserAgent);
        }
    }
    if let Some(overrides) = connection.local_proxy_request_overrides.as_ref() {
        if !overrides.body.is_null() && !overrides.body.is_object() {
            return Err(ValidationError::InvalidConnectionBody);
        }
        for (name, value) in &overrides.headers {
            if !is_valid_header_name(name) || value.chars().any(char::is_control) {
                return Err(ValidationError::InvalidConnectionHeader(name.clone()));
            }
        }
    }
    Ok(())
}

fn is_valid_header_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name == name.trim()
        && name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'!' | b'#'
                        | b'$'
                        | b'%'
                        | b'&'
                        | b'\''
                        | b'*'
                        | b'+'
                        | b'-'
                        | b'.'
                        | b'^'
                        | b'_'
                        | b'`'
                        | b'|'
                        | b'~'
                )
        })
}

fn validate_responses_options(
    protocol: UpstreamProtocol,
    options: Option<ResponsesOptions>,
) -> Result<(), ValidationError> {
    match (protocol, options) {
        (UpstreamProtocol::Responses, None) => Err(ValidationError::ResponsesRequiresOptions),
        (UpstreamProtocol::Responses, Some(_)) => Ok(()),
        (_, Some(_)) => Err(ValidationError::UnexpectedResponsesOptions),
        (_, None) => Ok(()),
    }
}

fn validate_model_options(
    app: AppKind,
    options: &ModelOptions,
    primary_model: Option<&str>,
) -> Result<(), ValidationError> {
    let reject = |kind: &'static str| {
        Err(ValidationError::ModelOptionsMismatch {
            options_kind: kind,
            app,
        })
    };
    match (app, options) {
        (AppKind::Codex, ModelOptions::Codex(settings)) => validate_codex_settings(settings),
        (AppKind::Claude, ModelOptions::Claude(settings)) => {
            validate_claude_settings(settings, primary_model)
        }
        (AppKind::Codex, ModelOptions::Claude(_)) => reject("claude"),
        (AppKind::Claude, ModelOptions::Codex(_)) => reject("codex"),
    }
}

fn validate_model_identifier(
    model: Option<&str>,
    field: &'static str,
) -> Result<(), ValidationError> {
    let Some(model) = model else {
        return Ok(());
    };
    if crate::claude_model::contains_one_m_marker(model) {
        return Err(ValidationError::InlineOneMMarker { field });
    }
    Ok(())
}

fn validate_codex_settings(settings: &CodexModelSettings) -> Result<(), ValidationError> {
    if let Some(window) = settings.context_window {
        if window == 0 {
            return Err(ValidationError::BadContextWindow);
        }
    }
    Ok(())
}

fn validate_claude_settings(
    settings: &ClaudeModelSettings,
    primary_model: Option<&str>,
) -> Result<(), ValidationError> {
    validate_model_identifier(settings.haiku_model.as_deref(), "Haiku 档")?;
    validate_model_identifier(settings.sonnet_model.as_deref(), "Sonnet 档")?;
    validate_model_identifier(settings.opus_model.as_deref(), "Opus 档")?;
    validate_model_identifier(settings.fable_model.as_deref(), "Fable 档")?;
    validate_model_identifier(settings.subagent_model.as_deref(), "子 agent 模型")?;
    validate_one_m_enabled(
        settings.fable_one_m,
        settings.fable_model.as_deref(),
        "Fable 档",
    )?;
    validate_one_m_enabled(
        settings.subagent_one_m,
        settings.subagent_model.as_deref(),
        "子 agent 模型",
    )?;
    if let Some(names) = &settings.display_names {
        for value in [&names.haiku, &names.sonnet, &names.opus, &names.fable]
            .into_iter()
            .flatten()
        {
            if value.trim().is_empty() || value.chars().any(char::is_control) {
                return Err(ValidationError::InvalidModelDisplayName);
            }
        }
    }
    validate_one_m_enabled(
        settings.haiku_one_m,
        settings.haiku_model.as_deref(),
        "Haiku 档",
    )?;
    validate_one_m_enabled(settings.primary_one_m, primary_model, "主模型")?;
    validate_one_m_enabled(
        settings.sonnet_one_m,
        settings.sonnet_model.as_deref(),
        "Sonnet 档",
    )?;
    validate_one_m_enabled(
        settings.opus_one_m,
        settings.opus_model.as_deref(),
        "Opus 档",
    )?;
    if let Some(models) = settings.available_models.as_deref() {
        for model in models {
            if model.trim().is_empty() {
                return Err(ValidationError::EmptyAvailableModel);
            }
            validate_model_identifier(Some(model), "可选模型列表")?;
        }
    }
    Ok(())
}

fn validate_one_m_enabled(
    enabled: bool,
    model: Option<&str>,
    field: &'static str,
) -> Result<(), ValidationError> {
    if enabled && model.is_none_or(|model| model.trim().is_empty()) {
        return Err(ValidationError::OneMRequiresModel { field });
    }
    Ok(())
}
