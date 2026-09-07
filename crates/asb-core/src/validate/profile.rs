use crate::contracts::{
    AppKind, ClaudeModelSettings, CodexModelSettings, ModelOptions, ProviderDraft, ProviderProfile,
    RouteMode, UpstreamProtocol, UsageQuery,
};

use crate::validate::error::{ValidationError, MAX_AUTO_REFRESH_INTERVAL_MINUTES, MAX_NOTES_LEN};
use crate::validate::validate_usage_query;

impl ProviderProfile {
    pub fn validate(&self) -> Result<(), ValidationError> {
        if self.id.trim().is_empty() {
            return Err(ValidationError::EmptyId);
        }
        validate_profile_fields(ProfileFields {
            app: self.app,
            route_mode: self.route_mode,
            name: &self.name,
            model: self.model.as_deref(),
            base_url: self.base_url.as_deref(),
            api_key: &self.api_key,
            upstream_protocol: self.upstream_protocol,
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
        validate_profile_fields(ProfileFields {
            app: self.app,
            route_mode: self.route_mode,
            name: &self.name,
            model: self.model.as_deref(),
            base_url: self.base_url.as_deref(),
            api_key: &self.api_key,
            upstream_protocol: self.upstream_protocol,
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
    api_key: &'a str,
    upstream_protocol: Option<UpstreamProtocol>,
    max_output_tokens: crate::contracts::ExplicitMaxOutputTokens,
    model_options: Option<&'a ModelOptions>,
    notes: Option<&'a str>,
    website_url: Option<&'a str>,
    usage_query: Option<&'a UsageQuery>,
    official_quota_refresh_interval: Option<u32>,
}

fn validate_profile_fields(fields: ProfileFields<'_>) -> Result<(), ValidationError> {
    let ProfileFields {
        app,
        route_mode,
        name,
        model,
        base_url,
        api_key,
        upstream_protocol,
        max_output_tokens,
        model_options,
        notes,
        website_url,
        usage_query,
        official_quota_refresh_interval,
    } = fields;

    if name.trim().is_empty() {
        return Err(ValidationError::EmptyName);
    }
    match route_mode {
        RouteMode::Official => {
            if base_url.is_some()
                || !api_key.trim().is_empty()
                || upstream_protocol.is_some()
                || max_output_tokens.is_some()
                || model.is_some()
                || model_options.is_some()
                || usage_query.is_some()
            {
                return Err(ValidationError::OfficialRouteHasCustomFields);
            }
            if official_quota_refresh_interval.is_some() && app != AppKind::Codex {
                return Err(ValidationError::QuotaIntervalRequiresOfficialCodex);
            }
        }
        RouteMode::Custom => {
            if official_quota_refresh_interval.is_some() {
                return Err(ValidationError::QuotaIntervalRequiresOfficialCodex);
            }
            let Some(url) = base_url else {
                return Err(ValidationError::CustomRequiresBaseUrl);
            };
            let ok = url.starts_with("https://") || url.starts_with("http://");
            if !ok {
                return Err(ValidationError::BadBaseUrl(url.to_string()));
            }
            if api_key.trim().is_empty() {
                return Err(ValidationError::EmptyApiKey);
            }
            if upstream_protocol.is_none() {
                return Err(ValidationError::CustomRequiresProtocol);
            }
            let requires_max_output_tokens = app == AppKind::Codex
                && upstream_protocol == Some(UpstreamProtocol::AnthropicMessages);
            if requires_max_output_tokens {
                if !max_output_tokens.value().is_some_and(|value| value > 0) {
                    return Err(ValidationError::CodexAnthropicRequiresMaxOutputTokens);
                }
            } else if max_output_tokens.is_some() {
                return Err(ValidationError::UnexpectedMaxOutputTokens);
            }
            if api_key.chars().count() > 4_096 {
                return Err(ValidationError::ApiKeyTooLong(4_096));
            }
        }
    }
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
