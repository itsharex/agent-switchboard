use serde::{Deserialize, Serialize};

use crate::contracts::{
    AppKind, ExplicitMaxOutputTokens, ModelOptions, RouteMode, UpstreamProtocol, UsageQuery,
};

/// A provider profile. It is a small overlay, never a full copy of a user's
/// configuration file. `route_mode` is the one routing owner: custom profiles
/// own an endpoint and API key; official profiles represent the client's
/// native login without copying any credential cache.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfile {
    pub id: String,
    pub app: AppKind,
    pub route_mode: RouteMode,
    pub name: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub api_key: String,
    /// Required for custom routes and absent for official routes. It tells the
    /// activation service whether the client connects directly or through the
    /// local protocol gateway.
    pub upstream_protocol: Option<UpstreamProtocol>,
    /// Required only when Codex converts a Responses request to an Anthropic
    /// Messages upstream that requires `max_tokens` even when Codex omits a
    /// per-request output limit. This is an explicit profile setting, never
    /// a gateway fallback.
    pub max_output_tokens: ExplicitMaxOutputTokens,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_options: Option<ModelOptions>,
    /// Local-only note; never written into any client configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Provider homepage, used for navigation only.
    pub website_url: Option<String>,
    /// Optional usage-balance query; application-side metadata that is never
    /// written into any client configuration.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_query: Option<UsageQuery>,
    /// Whole minutes between automatic re-queries of the official Codex
    /// subscription-quota panel; application-side metadata that is never
    /// written into any client configuration. Absent keeps the panel
    /// manual-only, so there is no separate zero representation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_quota_refresh_interval_minutes: Option<u32>,
}

/// Editable provider fields. The application assigns the stable profile id
/// when a draft is persisted. Routing mode is explicit; it is never inferred
/// from absent endpoint or credential fields.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderDraft {
    pub app: AppKind,
    pub route_mode: RouteMode,
    pub name: String,
    pub base_url: Option<String>,
    pub api_key: String,
    pub upstream_protocol: Option<UpstreamProtocol>,
    pub max_output_tokens: ExplicitMaxOutputTokens,
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_options: Option<ModelOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub website_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_query: Option<UsageQuery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_quota_refresh_interval_minutes: Option<u32>,
}

impl std::fmt::Debug for ProviderProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderProfile")
            .field("id", &self.id)
            .field("app", &self.app)
            .field("route_mode", &self.route_mode)
            .field("name", &self.name)
            .field("model", &self.model)
            .field("base_url", &self.base_url)
            .field("api_key", &crate::redact::REDACTED)
            .field("upstream_protocol", &self.upstream_protocol)
            .field("max_output_tokens", &self.max_output_tokens)
            .field("model_options", &self.model_options)
            .field("notes", &self.notes)
            .field("website_url", &self.website_url)
            .field("usage_query", &self.usage_query)
            .field(
                "official_quota_refresh_interval_minutes",
                &self.official_quota_refresh_interval_minutes,
            )
            .finish()
    }
}

impl std::fmt::Debug for ProviderDraft {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProviderDraft")
            .field("app", &self.app)
            .field("route_mode", &self.route_mode)
            .field("name", &self.name)
            .field("base_url", &self.base_url)
            .field("api_key", &crate::redact::REDACTED)
            .field("upstream_protocol", &self.upstream_protocol)
            .field("max_output_tokens", &self.max_output_tokens)
            .field("model", &self.model)
            .field("model_options", &self.model_options)
            .field("notes", &self.notes)
            .field("website_url", &self.website_url)
            .field("usage_query", &self.usage_query)
            .field(
                "official_quota_refresh_interval_minutes",
                &self.official_quota_refresh_interval_minutes,
            )
            .finish()
    }
}

impl ProviderProfile {
    pub fn from_draft(id: String, draft: ProviderDraft) -> Self {
        Self {
            id,
            app: draft.app,
            route_mode: draft.route_mode,
            name: draft.name,
            model: draft.model,
            base_url: draft.base_url,
            api_key: draft.api_key,
            upstream_protocol: draft.upstream_protocol,
            max_output_tokens: draft.max_output_tokens,
            model_options: draft.model_options,
            notes: draft.notes,
            website_url: draft.website_url,
            usage_query: draft.usage_query,
            official_quota_refresh_interval_minutes: draft.official_quota_refresh_interval_minutes,
        }
    }

    /// Whether persisting `draft` over this profile would change any field the
    /// client-file projection and the gateway route consume. Name, notes,
    /// website, usage query, and the official quota interval are
    /// application-side metadata and never qualify; this predicate is the
    /// single owner of that rule.
    pub fn draft_touches_live_configuration(&self, draft: &ProviderDraft) -> bool {
        self.route_mode != draft.route_mode
            || self.model != draft.model
            || self.base_url != draft.base_url
            || self.api_key != draft.api_key
            || self.upstream_protocol != draft.upstream_protocol
            || self.max_output_tokens != draft.max_output_tokens
            || self.model_options != draft.model_options
    }
}

/// What persisting one provider draft will do. This classification is the
/// single contract for the editor save flow; the interface never re-derives
/// which fields affect live configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ProfileSaveKind {
    /// A new provider: the store assigns the id; nothing is enabled.
    Create,
    /// The draft equals the stored profile; nothing is written.
    NoChange,
    /// Store the profile only. Either the change is metadata-only, or the
    /// provider is not currently active so live configuration cannot be
    /// affected.
    SaveOnly,
    /// The provider is currently active and effective parameters changed:
    /// store the profile and re-apply the client configuration in one
    /// transaction.
    SaveAndApply,
}

/// Classifies a provider draft against the stored profile and the observed
/// active identity. Callers pass `is_active: false` whenever the draft does
/// not touch live configuration, where the flag cannot change the result.
pub fn classify_profile_save(
    existing: Option<&ProviderProfile>,
    draft: &ProviderDraft,
    is_active: bool,
) -> ProfileSaveKind {
    let Some(profile) = existing else {
        return ProfileSaveKind::Create;
    };
    let candidate = ProviderProfile::from_draft(profile.id.clone(), draft.clone());
    if candidate == *profile {
        return ProfileSaveKind::NoChange;
    }
    if is_active && profile.draft_touches_live_configuration(draft) {
        return ProfileSaveKind::SaveAndApply;
    }
    ProfileSaveKind::SaveOnly
}

/// One provider's persisted file: the complete managed state of exactly one
/// supplier, stored as `providers/{client}/{id}.json`. The client association
/// is owned by the containing directory, so the file itself carries no `app`
/// field, and the sort position lives in the file so no separate index
/// exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderFile {
    /// Stable UUID; must equal the file name's stem.
    pub id: String,
    pub name: String,
    /// Sort position within the client's provider list.
    pub position: u64,
    pub route_mode: RouteMode,
    pub api_key: String,
    pub upstream_protocol: Option<UpstreamProtocol>,
    pub max_output_tokens: ExplicitMaxOutputTokens,
    pub base_url: Option<String>,
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_options: Option<ModelOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub website_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage_query: Option<UsageQuery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub official_quota_refresh_interval_minutes: Option<u32>,
}

impl ProviderFile {
    /// Attaches the client association owned by the storage directory.
    pub fn into_profile(self, app: AppKind) -> ProviderProfile {
        ProviderProfile {
            id: self.id,
            app,
            route_mode: self.route_mode,
            name: self.name,
            model: self.model,
            base_url: self.base_url,
            api_key: self.api_key,
            upstream_protocol: self.upstream_protocol,
            max_output_tokens: self.max_output_tokens,
            model_options: self.model_options,
            notes: self.notes,
            website_url: self.website_url,
            usage_query: self.usage_query,
            official_quota_refresh_interval_minutes: self.official_quota_refresh_interval_minutes,
        }
    }

    /// Strips the client association and records the file's sort position.
    pub fn from_profile(profile: &ProviderProfile, position: u64) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
            position,
            route_mode: profile.route_mode,
            api_key: profile.api_key.clone(),
            base_url: profile.base_url.clone(),
            model: profile.model.clone(),
            upstream_protocol: profile.upstream_protocol,
            max_output_tokens: profile.max_output_tokens,
            model_options: profile.model_options.clone(),
            notes: profile.notes.clone(),
            website_url: profile.website_url.clone(),
            usage_query: profile.usage_query.clone(),
            official_quota_refresh_interval_minutes: profile
                .official_quota_refresh_interval_minutes,
        }
    }
}

/// One provider profile together with the storage revision of its file. The
/// revision lets an editor refuse to overwrite an externally changed provider
/// file, mirroring the optimistic check used for common settings.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRecord {
    pub profile: ProviderProfile,
    pub file_hash: String,
}
