use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// A source-compatible endpoint entry imported from the source application.
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
    /// Explicit model-list endpoint used only by discovery. It never changes
    /// the request route or the active-provider routing identity.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub models_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalProxyRequestOverrides {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub body: Value,
}

/// Header names a provider override may never replace: HTTP framing,
/// regenerated credentials, official account identity, and tracing hops.
/// One owner for the save-time validator and the gateway's outbound gate.
pub const PROTECTED_OVERRIDE_HEADERS: [&str; 26] = [
    "host",
    "content-length",
    "content-type",
    "transfer-encoding",
    "connection",
    "accept-encoding",
    "authorization",
    "x-api-key",
    "x-goog-api-key",
    "proxy-authorization",
    "proxy-authenticate",
    "te",
    "trailer",
    "upgrade",
    "chatgpt-account-id",
    "session_id",
    "x-client-request-id",
    "x-forwarded-host",
    "x-forwarded-port",
    "x-forwarded-proto",
    "forwarded",
    "x-request-id",
    "x-correlation-id",
    "x-trace-id",
    "traceparent",
    "tracestate",
];

fn is_header_token(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
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

fn is_header_value(value: &str) -> bool {
    value.len() <= 4096
        && value
            .bytes()
            .all(|byte| byte == b'\t' || (0x20..=0x7E).contains(&byte) || byte >= 0x80)
}

impl LocalProxyRequestOverrides {
    /// Rejects overrides the outbound gate would silently drop, so invalid
    /// input fails at save time instead.
    pub fn validate(&self) -> Result<(), String> {
        for (name, value) in &self.headers {
            let name = name.trim();
            if !is_header_token(name) {
                return Err(format!("请求头覆盖的名字不是合法 HTTP 头名：{name}"));
            }
            if PROTECTED_OVERRIDE_HEADERS
                .iter()
                .any(|protected| protected.eq_ignore_ascii_case(name))
            {
                return Err(format!("请求头覆盖不能替换受保护的头：{name}"));
            }
            if !is_header_value(value) {
                return Err(format!("请求头覆盖的值含有控制字符或过长：{name}"));
            }
        }
        if !self.body.is_null() && !self.body.is_object() {
            return Err("请求体覆盖必须是 JSON 对象".into());
        }
        Ok(())
    }
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

    /// Validates the custom User-Agent and the request overrides so invalid
    /// values fail at save/import time instead of being dropped at egress.
    /// Validates the independent model-discovery endpoint. A full request URL
    /// cannot safely imply a GET model-list endpoint, so callers must use this
    /// explicit field when discovery needs a different URL.
    pub fn validate_models_url(&self) -> Result<(), String> {
        if let Some(url) = &self.models_url {
            crate::endpoint::validate_full_url(url)?;
        }
        Ok(())
    }

    pub fn validate_request_overrides(&self) -> Result<(), String> {
        if let Some(user_agent) = &self.custom_user_agent {
            let user_agent = user_agent.trim();
            if user_agent.is_empty() {
                return Err("自定义 User-Agent 不能为空".into());
            }
            if user_agent.len() > 512
                || !user_agent
                    .bytes()
                    .all(|byte| (0x20..=0x7E).contains(&byte) || byte >= 0x80)
            {
                return Err("自定义 User-Agent 含有控制字符或超过 512 字节".into());
            }
        }
        if let Some(overrides) = &self.local_proxy_request_overrides {
            overrides.validate()?;
        }
        Ok(())
    }

    pub fn requires_gateway(&self, base_url: Option<&str>) -> bool {
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
            || (self.endpoint_auto_select != Some(false)
                && self.has_distinct_endpoint_candidate(base_url))
    }

    /// Whether automatic endpoint selection has a target that differs from the
    /// profile's base URL. Candidates equal to the base URL offer automatic
    /// routing nothing to choose between, so they alone never justify the
    /// local gateway.
    pub fn has_distinct_endpoint_candidate(&self, base_url: Option<&str>) -> bool {
        let normalized_base = base_url.map(Self::normalize_endpoint_url);
        self.custom_endpoints
            .keys()
            .any(|url| Some(Self::normalize_endpoint_url(url)) != normalized_base)
    }

    /// Returns the stable URL spelling used by endpoint maps and routing.
    ///
    /// Endpoint metadata comes from both the source application imports and the local
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
    /// Custom targets use the same stable preference order as the source application:
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

#[cfg(test)]
mod tests {
    use super::*;

    fn options_with_endpoints(
        urls: &[&str],
        auto_select: Option<bool>,
    ) -> ProviderConnectionOptions {
        ProviderConnectionOptions {
            custom_endpoints: urls
                .iter()
                .map(|url| {
                    (
                        url.to_string(),
                        ProviderEndpoint {
                            url: url.to_string(),
                            added_at: 1,
                            last_used: None,
                        },
                    )
                })
                .collect(),
            endpoint_auto_select: auto_select,
            ..Default::default()
        }
    }

    #[test]
    fn candidates_identical_to_the_base_url_do_not_require_the_gateway() {
        let options = options_with_endpoints(&["https://upstream.example/api"], Some(true));
        assert!(!options.requires_gateway(Some("https://upstream.example/api/")));
    }

    #[test]
    fn a_distinct_candidate_still_requires_the_gateway() {
        let options = options_with_endpoints(
            &["https://upstream.example/api", "https://mirror.example/api"],
            Some(true),
        );
        assert!(options.requires_gateway(Some("https://upstream.example/api")));
        assert!(options.requires_gateway(None));
    }

    #[test]
    fn auto_select_disabled_keeps_direct_routing_regardless_of_candidates() {
        let options = options_with_endpoints(&["https://mirror.example/api"], Some(false));
        assert!(!options.requires_gateway(Some("https://upstream.example/api")));
    }
}
