//! Provider URL semantics shared by validation, model discovery and requests.
//!
//! OpenAI-compatible base URLs include the provider's complete API prefix.
//! Anthropic base URLs are service roots, matching Claude Code's SDK contract.

use crate::contracts::UpstreamProtocol;
use url::Url;

pub fn validate_base_url(base_url: &str, protocol: UpstreamProtocol) -> Result<(), String> {
    parse_base(base_url, protocol).map(|_| ())
}

pub fn validate_full_url(endpoint: &str) -> Result<(), String> {
    parse_full_url(endpoint).map(|_| ())
}

pub fn upstream_endpoint(base_url: &str, protocol: UpstreamProtocol) -> Result<String, String> {
    if protocol == UpstreamProtocol::GeminiGenerateContent {
        return Err("Gemini 原生请求需要模型和流式模式，请使用 Claude 模型端点解析".into());
    }
    append(base_url, protocol, request_path(protocol))
}

/// Resolves a provider request target while honoring CC Switch's full-URL
/// flag. Full URLs are used verbatim after validation; no protocol path is
/// appended a second time.
pub fn upstream_endpoint_with_options(
    base_url: &str,
    protocol: UpstreamProtocol,
    is_full_url: bool,
) -> Result<String, String> {
    if is_full_url {
        return parse_full_url(base_url);
    }
    upstream_endpoint(base_url, protocol)
}

/// The native Codex compact endpoint. It is deliberately separate from a
/// generated Responses request: callers must not turn a provider-supported
/// compact operation into a normal model summary.
pub fn compact_endpoint(base_url: &str) -> Result<String, String> {
    append(base_url, UpstreamProtocol::Responses, "/responses/compact")
}

/// A sibling endpoint exposed by a Codex-compatible OpenAI API root.
///
/// The caller must supply one of its fixed protocol paths. Keeping URL
/// assembly here prevents individual gateway operations from treating a
/// configured API root as an arbitrary forwarding URL.
pub fn codex_endpoint(base_url: &str, path: &str) -> Result<String, String> {
    if !path.starts_with('/') || path.contains('?') || path.contains('#') {
        return Err("Codex 操作路径无效".to_string());
    }
    append(base_url, UpstreamProtocol::Responses, path)
}

pub fn models_endpoint(base_url: &str, protocol: UpstreamProtocol) -> Result<String, String> {
    let suffix = match protocol {
        UpstreamProtocol::Responses | UpstreamProtocol::ChatCompletions => "/models",
        UpstreamProtocol::AnthropicMessages => "/v1/models",
        UpstreamProtocol::GeminiGenerateContent => {
            return crate::claude_gemini::models_endpoint(base_url, false)
        }
    };
    append(base_url, protocol, suffix)
}

/// Resolves the model-list target using the same full-URL semantics as the
/// request endpoint. A full URL is intentionally not guessed or rewritten.
pub fn models_endpoint_for_connection(
    base_url: &str,
    protocol: UpstreamProtocol,
    connection: &crate::contracts::ProviderConnectionOptions,
) -> Result<String, String> {
    if let Some(url) = &connection.claude_models_url {
        validate_full_url(url)?;
        return Ok(url.clone());
    }
    if protocol == UpstreamProtocol::GeminiGenerateContent {
        return crate::claude_gemini::models_endpoint(base_url, connection.is_full_url);
    }
    models_endpoint_with_options(base_url, protocol, connection.is_full_url)
}

pub fn models_endpoint_with_options(
    base_url: &str,
    protocol: UpstreamProtocol,
    is_full_url: bool,
) -> Result<String, String> {
    if is_full_url {
        return parse_full_url(base_url);
    }
    models_endpoint(base_url, protocol)
}

fn request_path(protocol: UpstreamProtocol) -> &'static str {
    match protocol {
        UpstreamProtocol::Responses => "/responses",
        UpstreamProtocol::ChatCompletions => "/chat/completions",
        UpstreamProtocol::AnthropicMessages => "/v1/messages",
        UpstreamProtocol::GeminiGenerateContent => ":generateContent",
    }
}

fn parse_base(base_url: &str, protocol: UpstreamProtocol) -> Result<Url, String> {
    let invalid = || "服务地址必须是无凭据、查询参数或片段的完整 HTTP(S) 地址".to_string();
    let authority = base_url.split_once("://").map(|(_, rest)| rest);
    if base_url.trim() != base_url
        || base_url.chars().any(char::is_control)
        || base_url.contains('\\')
        || authority.is_none_or(|rest| rest.is_empty() || rest.starts_with('/'))
    {
        return Err(invalid());
    }
    let url = Url::parse(base_url).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    if url
        .path()
        .trim_end_matches('/')
        .ends_with(request_path(protocol))
    {
        return Err(format!(
            "服务地址应填写{}，不要包含完整请求端点 {}",
            if protocol == UpstreamProtocol::AnthropicMessages {
                "服务根地址"
            } else {
                "API 根地址（含服务商要求的前缀）"
            },
            request_path(protocol),
        ));
    }
    Ok(url)
}

fn parse_full_url(endpoint: &str) -> Result<String, String> {
    let invalid = || "完整服务地址必须是无凭据、片段或控制字符的 HTTP(S) 地址".to_string();
    if endpoint.trim() != endpoint
        || endpoint.chars().any(char::is_control)
        || endpoint.contains('\\')
    {
        return Err(invalid());
    }
    let url = Url::parse(endpoint).map_err(|_| invalid())?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(url.to_string())
}

fn append(base_url: &str, protocol: UpstreamProtocol, suffix: &str) -> Result<String, String> {
    let mut url = parse_base(base_url, protocol)?;
    let path = format!("{}{suffix}", url.path().trim_end_matches('/'));
    url.set_path(&path);
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_paths_preserve_the_declared_api_prefix_without_guessing_versions() {
        for (base, prefix) in [
            ("https://example.test", "https://example.test"),
            ("https://example.test/", "https://example.test"),
            ("https://example.test/v1/", "https://example.test/v1"),
            ("https://example.test/v2", "https://example.test/v2"),
            (
                "https://example.test/proxy/openai/",
                "https://example.test/proxy/openai",
            ),
            ("http://[::1]:9876/api", "http://[::1]:9876/api"),
            (
                "https://example.test/tenant%20name",
                "https://example.test/tenant%20name",
            ),
        ] {
            for (protocol, path) in [
                (UpstreamProtocol::Responses, "responses"),
                (UpstreamProtocol::ChatCompletions, "chat/completions"),
            ] {
                assert_eq!(
                    upstream_endpoint(base, protocol).unwrap(),
                    format!("{prefix}/{path}")
                );
                assert_eq!(
                    models_endpoint(base, protocol).unwrap(),
                    format!("{prefix}/models")
                );
            }
        }
    }

    #[test]
    fn anthropic_paths_match_the_client_service_root_contract() {
        let protocol = UpstreamProtocol::AnthropicMessages;
        assert_eq!(
            upstream_endpoint("https://example.test/proxy/", protocol).unwrap(),
            "https://example.test/proxy/v1/messages"
        );
        assert_eq!(
            models_endpoint("https://example.test/proxy/", protocol).unwrap(),
            "https://example.test/proxy/v1/models"
        );
    }

    #[test]
    fn request_endpoints_are_not_accepted_as_base_url_aliases() {
        for protocol in [
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            UpstreamProtocol::AnthropicMessages,
        ] {
            let endpoint = upstream_endpoint("https://example.test/api", protocol).unwrap();
            assert!(validate_base_url(&endpoint, protocol).is_err());
            assert!(validate_base_url(&format!("{endpoint}/"), protocol).is_err());
        }
    }

    #[test]
    fn compact_endpoint_uses_the_declared_responses_api_root() {
        assert_eq!(
            compact_endpoint("https://example.test/tenant/v1").unwrap(),
            "https://example.test/tenant/v1/responses/compact"
        );
    }

    #[test]
    fn codex_operation_endpoints_are_siblings_of_the_declared_api_root() {
        assert_eq!(
            codex_endpoint("https://example.test/tenant/v1", "/images/generations").unwrap(),
            "https://example.test/tenant/v1/images/generations"
        );
        assert!(codex_endpoint("https://example.test/v1", "models?all=true").is_err());
    }

    #[test]
    fn invalid_addresses_are_rejected_without_echoing_sensitive_input() {
        for base in [
            "https://",
            "https:///path",
            "https:example.test",
            "https://example.test\\path",
            "ftp://example.test",
            "https://user:secret@example.test",
            "https://example.test?token=secret",
            "https://example.test#secret",
            " https://example.test",
            "https://example.test/\nsecret",
        ] {
            let error = validate_base_url(base, UpstreamProtocol::Responses).unwrap_err();
            assert!(!error.contains("secret"));
        }
    }

    #[test]
    fn full_urls_are_not_extended_with_protocol_paths() {
        assert_eq!(
            upstream_endpoint_with_options(
                "https://example.test/custom/messages",
                UpstreamProtocol::AnthropicMessages,
                true,
            )
            .unwrap(),
            "https://example.test/custom/messages"
        );
        assert_eq!(
            models_endpoint_with_options(
                "https://example.test/custom/messages",
                UpstreamProtocol::AnthropicMessages,
                true,
            )
            .unwrap(),
            "https://example.test/custom/messages"
        );
    }

    #[test]
    fn full_urls_preserve_safe_query_and_reject_credentials_or_fragments() {
        assert_eq!(
            upstream_endpoint_with_options(
                "https://example.test/messages?tenant=one%20two",
                UpstreamProtocol::AnthropicMessages,
                true,
            )
            .unwrap(),
            "https://example.test/messages?tenant=one%20two"
        );
        for endpoint in [
            "https://user:secret@example.test/messages",
            "https://example.test/messages#secret",
        ] {
            let error =
                upstream_endpoint_with_options(endpoint, UpstreamProtocol::AnthropicMessages, true)
                    .unwrap_err();
            assert!(!error.contains("secret"));
        }
    }
}
