//! Provider URL semantics shared by validation, model discovery and requests.
//!
//! OpenAI-compatible base URLs include the provider's complete API prefix.
//! Anthropic base URLs are service roots, matching Claude Code's SDK contract.

use crate::contracts::UpstreamProtocol;
use url::Url;

pub fn validate_base_url(base_url: &str, protocol: UpstreamProtocol) -> Result<(), String> {
    parse_base(base_url, protocol).map(|_| ())
}

pub fn upstream_endpoint(base_url: &str, protocol: UpstreamProtocol) -> Result<String, String> {
    append(base_url, protocol, request_path(protocol))
}

pub fn models_endpoint(base_url: &str, protocol: UpstreamProtocol) -> Result<String, String> {
    let suffix = match protocol {
        UpstreamProtocol::Responses | UpstreamProtocol::ChatCompletions => "/models",
        UpstreamProtocol::AnthropicMessages => "/v1/models",
    };
    append(base_url, protocol, suffix)
}

fn request_path(protocol: UpstreamProtocol) -> &'static str {
    match protocol {
        UpstreamProtocol::Responses => "/responses",
        UpstreamProtocol::ChatCompletions => "/chat/completions",
        UpstreamProtocol::AnthropicMessages => "/v1/messages",
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
}
