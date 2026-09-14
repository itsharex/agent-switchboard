use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// A source-compatible endpoint entry imported from CC Switch.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderEndpoint {
    pub url: String,
    pub added_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used: Option<i64>,
}

/// Optional connection behavior owned by a provider profile but never
/// rendered into Claude settings.json.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderConnectionOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claude_native: Option<crate::claude_native::ClaudeNative>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex: Option<super::CodexRequestOptions>,
    /// Treat base_url and custom endpoint entries as complete request URLs.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_full_url: bool,
    /// Additional request targets, keyed by their normalized URL.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub custom_endpoints: BTreeMap<String, ProviderEndpoint>,
    /// Whether endpoint candidates should be considered by automatic routing.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint_auto_select: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_user_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_proxy_request_overrides: Option<LocalProxyRequestOverrides>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_binding: Option<ProviderAuthBinding>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_field: Option<ClaudeApiKeyField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_billing: Option<super::ClaudeBilling>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claude_models_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalProxyRequestOverrides {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub body: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderAuthBinding {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClaudeApiKeyField {
    #[serde(rename = "ANTHROPIC_AUTH_TOKEN")]
    AnthropicAuthToken,
    #[serde(rename = "ANTHROPIC_API_KEY")]
    AnthropicApiKey,
}

fn is_false(value: &bool) -> bool {
    !value
}

impl ProviderConnectionOptions {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn requires_gateway(&self) -> bool {
        self.provider_type.is_some()
            || self
                .auth_binding
                .as_ref()
                .is_some_and(|binding| binding.source == "managed_account")
            || self.claude_billing.is_some()
            || self.claude_prompt_cache_key.is_some()
            || self.is_full_url
            || self.custom_user_agent.is_some()
            || self
                .local_proxy_request_overrides
                .as_ref()
                .is_some_and(|overrides| !overrides.headers.is_empty() || !overrides.body.is_null())
            || (self.endpoint_auto_select != Some(false) && !self.custom_endpoints.is_empty())
    }

    /// Returns the stable URL spelling used by endpoint maps and routing.
    ///
    /// Endpoint metadata comes from both CC Switch imports and the local
    /// editor. Treating a trailing slash as significant would create duplicate
    /// candidates for the same upstream and would make route fingerprints
    /// change during harmless normalization.
    pub fn normalize_endpoint_url(url: &str) -> String {
        url.trim().trim_end_matches('/').to_string()
    }

    /// The connection fields that affect request routing. Usage timestamps
    /// and insertion times are deliberately excluded: they are scheduling
    /// metadata, not a different upstream capability.
    pub fn routing_identity(&self) -> Value {
        let custom_endpoints = self
            .custom_endpoints
            .keys()
            .map(|url| Self::normalize_endpoint_url(url))
            .filter(|url| !url.is_empty())
            .collect::<Vec<_>>();
        let mut identity = serde_json::json!({
            "isFullUrl": self.is_full_url,
            "customEndpoints": custom_endpoints,
            "endpointAutoSelect": self.endpoint_auto_select,
            "customUserAgent": self.custom_user_agent,
            "localProxyRequestOverrides": self.local_proxy_request_overrides,
            "authBinding": self.auth_binding,
            "providerType": self.provider_type,
            "apiKeyField": self.api_key_field,
            "claudePromptCacheKey": self.claude_prompt_cache_key,
            "claudeNative": self.claude_native,
        });
        if let Some(options) = &self.codex {
            identity["codex"] = serde_json::json!(options);
        }
        identity
    }

    /// The configured primary URL followed by source-managed custom targets.
    /// Duplicate URLs are removed while preserving the primary-first order.
    /// Custom targets use the same stable preference order as CC Switch:
    /// recently used targets first, then newer targets, then URL order.
    pub fn endpoint_candidates(&self, base_url: &str) -> Vec<String> {
        let mut candidates: Vec<String> = Vec::new();
        let mut push = |url: &str| {
            let url = Self::normalize_endpoint_url(url);
            if !url.is_empty() && !candidates.iter().any(|candidate| candidate == &url) {
                candidates.push(url);
            }
        };
        push(base_url);
        if self.endpoint_auto_select != Some(false) {
            let mut endpoints = self.custom_endpoints.iter().collect::<Vec<_>>();
            endpoints.sort_by(|(left_url, left), (right_url, right)| {
                right
                    .last_used
                    .cmp(&left.last_used)
                    .then_with(|| right.added_at.cmp(&left.added_at))
                    .then_with(|| left_url.cmp(right_url))
            });
            for (key, _) in endpoints {
                push(key);
            }
        }
        candidates
    }
}
