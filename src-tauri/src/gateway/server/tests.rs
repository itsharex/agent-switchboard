mod claude_accounts;
mod claude_cli;
mod claude_failover;
mod codex_cli;
mod codex_compact_cli;
mod codex_failover;
mod codex_metering;
mod codex_official_takeover;
mod compaction;
mod lifecycle;
mod model_budgets;
mod native_compaction;
mod operations;
mod protocol_pairs;
mod provider_failures;
mod query;
mod request_overrides;
mod route_revisions;
mod streaming;
mod transport_encoding;
mod websocket_roundtrip;

use super::respond::content_type;
use super::*;
use crate::gateway::GatewayController;
use crate::local_state::LocalState;
use asb_core::contracts::{
    AppKind, AuthenticationScheme, CodexCapabilities, CodexCatalogEntry, CodexChatEffortMode,
    CodexChatEffortParameter, CodexChatReasoning, CodexChatThinkingParameter, CodexEndpoint,
    CodexModelRoute, CodexProviderDraft, CodexProviderFile, CodexRouteMode, CodexUpstream,
    ConfigValue, ProviderDraft, RouteMode, SettingValue, SwitchPlan, UpstreamProtocol,
};
use asb_core::ownership::default_client_settings;
use asb_switch::io::FsIo;
use reqwest::header::CONTENT_TYPE;
use serde_json::{json, Value};
use std::fs;
use std::io::Read;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;
use tiny_http::Response;

fn endpoint(server: &Server) -> String {
    let port = server
        .server_addr()
        .to_ip()
        .expect("loopback address")
        .port();
    format!("http://127.0.0.1:{port}")
}

pub(crate) fn sandbox_codex_file(
    state: &LocalState,
    name: &str,
    upstream_url: String,
    api_key: String,
    upstream: CodexUpstream,
) -> CodexProviderFile {
    let endpoint = if upstream == CodexUpstream::AnthropicMessages {
        upstream_url
    } else {
        format!("{upstream_url}/v1")
    };
    let chat_reasoning = if upstream == CodexUpstream::ChatCompletions {
        CodexChatReasoning::Configured {
            thinking_parameter: CodexChatThinkingParameter::None,
            effort_parameter: CodexChatEffortParameter::ReasoningEffort,
            effort_mode: CodexChatEffortMode::LowHigh,
        }
    } else {
        CodexChatReasoning::Unsupported
    };
    let record = state
        .configuration()
        .create_codex_provider(CodexProviderDraft {
            name: name.to_string(),
            endpoint: CodexEndpoint(endpoint),
            api_key,
            authentication: None,
            connection: Default::default(),
            upstream,
            request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
            default_model: "sandbox-model".to_string(),
            catalog: vec![CodexCatalogEntry {
                id: "sandbox-model".to_string(),
                context_window: 128_000,
                max_output_tokens: 16_384,
                function_tools: true,
                custom_tools: true,
                tool_search: true,
                reasoning: true,
                default_reasoning_level: asb_core::contracts::CodexReasoningLevel::High,
                supported_reasoning_levels: vec![
                    asb_core::contracts::CodexReasoningLevel::None,
                    asb_core::contracts::CodexReasoningLevel::High,
                ],
                images: true,
                compact: true,
                display_name: None,
                description: None,
                base_instructions: None,
                supports_parallel_tool_calls: None,
            }],
            model_routes: vec![CodexModelRoute {
                client_model: "sandbox-model".to_string(),
                upstream_model: "sandbox-model".to_string(),
            }],
            capabilities: CodexCapabilities {
                responses: true,
                compact: true,
                models: true,
                chat_completions: true,
                alpha_search: true,
                image_generation: true,
                image_edit: true,
                function_tools: true,
                custom_tools: true,
                tool_search: true,
                reasoning: true,
                chat_reasoning,
            },
            parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
            notes: None,
            website_url: None,
            usage_query: None,
        })
        .expect("create Codex provider");
    let mut file = state
        .configuration()
        .find_codex_provider_file(&record.profile.id)
        .expect("load Codex provider");
    // These server fixtures exercise the loopback gateway. Production-created
    // Responses profiles default to direct activation, so the test intent is
    // explicit here rather than inherited from the production default.
    file.profile.connection.custom_user_agent = Some("ASB gateway fixture".to_string());
    file.profile.route_mode = CodexRouteMode::Gateway;
    state
        .configuration()
        .update_codex_provider_file(file.clone(), &record.file_hash)
        .expect("persist Codex gateway fixture");
    file
}

fn codex_upstream(protocol: UpstreamProtocol) -> CodexUpstream {
    match protocol {
        asb_core::UpstreamProtocol::GeminiGenerateContent => {
            panic!("Google native has a separate Claude fixture")
        }
        UpstreamProtocol::Responses => CodexUpstream::Responses,
        UpstreamProtocol::ChatCompletions => CodexUpstream::ChatCompletions,
        UpstreamProtocol::AnthropicMessages => CodexUpstream::AnthropicMessages,
    }
}

fn sandbox_profile(
    state: &LocalState,
    app: AppKind,
    name: &str,
    upstream_url: String,
    api_key: String,
    upstream_protocol: UpstreamProtocol,
) -> asb_core::ProviderProfile {
    state
        .configuration()
        .create_provider(ProviderDraft {
            authentication: None,
            parameters: asb_core::ownership::default_provider_parameters(app),
            claude_fragment: Default::default(),
            app,
            route_mode: RouteMode::Custom,
            name: name.to_string(),
            base_url: Some(
                if upstream_protocol == UpstreamProtocol::AnthropicMessages {
                    upstream_url
                } else {
                    format!("{upstream_url}/v1")
                },
            ),
            connection: Default::default(),
            api_key,
            upstream_protocol: Some(upstream_protocol),
            responses_options: (Some(upstream_protocol)
                == Some(asb_core::contracts::UpstreamProtocol::Responses))
            .then_some(asb_core::contracts::ResponsesOptions {
                request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
            }),
            max_output_tokens: ((app == AppKind::Codex
                && upstream_protocol == UpstreamProtocol::AnthropicMessages)
                .then_some(8_192))
            .into(),
            model: Some("sandbox-model".to_string()),
            model_options: None,
            notes: None,
            website_url: None,
            usage_query: None,
            display: None,
            official_quota_refresh_interval_minutes: None,
        })
        .expect("create provider")
        .profile
}

fn projection_token(projection: &crate::gateway::GatewayProjection) -> String {
    match &projection.activation {
        crate::gateway::GatewayActivation::Routed(route) => route.client_token.clone(),
        _ => panic!("test requires an activated gateway route"),
    }
}

fn assert_metric_attribution(
    gateway: &GatewayController,
    state: &LocalState,
    projection: &crate::gateway::GatewayProjection,
    expected_status: Option<u16>,
) {
    let observation = gateway.observe(state);
    let sample = observation
        .metrics
        .samples
        .last()
        .expect("completed gateway metric");
    let crate::gateway::GatewayActivation::Routed(route) = &projection.activation else {
        panic!("test requires an activated gateway route");
    };
    assert_eq!(
        sample.profile_id.as_deref(),
        Some(route.profile_id.as_str())
    );
    assert_eq!(
        sample.route_revision.as_deref(),
        Some(route.fingerprint.as_str())
    );
    assert_eq!(sample.upstream_protocol, Some(route.upstream_protocol));
    assert_eq!(sample.status, expected_status);
}

fn assert_metric_statuses_for_projection(
    gateway: &GatewayController,
    state: &LocalState,
    projection: &crate::gateway::GatewayProjection,
    expected_statuses: &[Option<u16>],
) {
    let crate::gateway::GatewayActivation::Routed(route) = &projection.activation else {
        panic!("test requires an activated gateway route");
    };
    let samples = gateway.observe(state).metrics.samples;
    let attributed: Vec<_> = samples
        .iter()
        .filter(|sample| {
            sample.profile_id.as_deref() == Some(route.profile_id.as_str())
                && sample.route_revision.as_deref() == Some(route.fingerprint.as_str())
        })
        .collect();
    assert_eq!(
        attributed
            .iter()
            .map(|sample| sample.status)
            .collect::<Vec<_>>(),
        expected_statuses
    );
}

fn codex_endpoint(projection: &crate::gateway::GatewayProjection) -> String {
    format!(
        "{}/responses",
        projection
            .plan
            .client_base_url()
            .expect("Codex gateway endpoint")
    )
}

mod claude_switching;
