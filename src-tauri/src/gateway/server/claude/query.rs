//! Claude SDK query hints are not OpenAI request options.

use super::*;

pub(super) fn request_url(
    route: &ActiveRoute,
    gateway_base: &str,
    incoming: &str,
    model: Option<&str>,
    stream: bool,
) -> Result<String, String> {
    // Validate the original path before filtering any query hints.
    let base = if route.upstream_protocol == UpstreamProtocol::GeminiGenerateContent {
        asb_core::claude_gemini::request_endpoint(
            &route.upstream_base_url,
            route.connection.is_full_url,
            model.ok_or("Gemini 请求缺少模型")?,
            stream,
        )?
    } else {
        asb_core::endpoint::upstream_endpoint_with_options(
            &route.upstream_base_url,
            route.upstream_protocol,
            route.connection.is_full_url,
        )?
    };
    let original = super::super::route::with_request_query(base.clone(), gateway_base, incoming)?;
    if route.upstream_protocol == UpstreamProtocol::AnthropicMessages {
        return Ok(original);
    }
    let Some((path, query)) = incoming.split_once('?') else {
        return Ok(original);
    };
    let kept = query
        .split('&')
        .filter(|pair| {
            let key = pair.split('=').next();
            key != Some("beta")
                && (route.upstream_protocol != UpstreamProtocol::GeminiGenerateContent
                    || key != Some("alt"))
        })
        .collect::<Vec<_>>();
    let filtered = if kept.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{}", kept.join("&"))
    };
    super::super::route::with_request_query(base, gateway_base, &filtered)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn route(protocol: UpstreamProtocol) -> ActiveRoute {
        ActiveRoute {
            app: AppKind::Claude,
            profile_id: "test".into(),
            fingerprint: "test".into(),
            client_token: "test".into(),
            continuation_key: [0; 32],
            upstream_base_url: "https://vendor.example/final?beta=provider&tenant=chosen".into(),
            connection: asb_core::contracts::ProviderConnectionOptions {
                is_full_url: true,
                ..Default::default()
            },
            upstream_protocol: protocol,
            responses_options: None,
            max_output_tokens: None,
            api_key: "fake-key".into(),
            authentication: asb_core::AuthenticationScheme::Bearer,
            claude_primary_model: None,
            claude_model_options: None,
            claude_account: None,
            codex_account: None,
            claude_fragment: Default::default(),
            codex: None,
        }
    }
    #[test]
    fn filters_only_client_beta_hints_across_protocols_and_keeps_provider_options() {
        let route = route(UpstreamProtocol::ChatCompletions);
        assert_eq!(
            request_url(
                &route,
                "http://127.0.0.1:47821",
                "/v1/messages?beta=true&trace=a%20b",
                None,
                false
            )
            .unwrap(),
            "https://vendor.example/final?beta=provider&tenant=chosen&trace=a%20b"
        );
    }
    #[test]
    fn native_anthropic_queries_are_preserved_and_malformed_queries_still_fail() {
        let route = route(UpstreamProtocol::AnthropicMessages);
        assert!(request_url(
            &route,
            "http://127.0.0.1:47821",
            "/v1/messages?beta=true",
            None,
            false
        )
        .unwrap()
        .ends_with("&beta=true"));
        assert!(request_url(
            &route,
            "http://127.0.0.1:47821",
            "/v1/messages?beta=x#bad",
            None,
            false
        )
        .is_err());
    }
}
