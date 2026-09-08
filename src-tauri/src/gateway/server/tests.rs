mod claude_cli;
mod codex_cli;
mod codex_compact_cli;
mod compaction;
mod lifecycle;
mod protocol_pairs;
mod provider_failures;
mod streaming;
mod websocket_roundtrip;

use super::respond::content_type;
use super::*;
use crate::gateway::GatewayController;
use crate::local_state::LocalState;
use asb_core::contracts::{
    AppKind, AuthenticationScheme, ConfigValue, ProviderDraft, RouteMode, SettingValue, SwitchPlan,
    UpstreamProtocol,
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
            parameters: asb_core::ownership::default_provider_parameters(app),
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

fn codex_endpoint(projection: &crate::gateway::GatewayProjection) -> String {
    format!(
        "{}/responses",
        projection
            .plan
            .client_base_url()
            .expect("Codex gateway endpoint")
    )
}
