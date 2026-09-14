use super::*;
use crate::claude_auth::{ClaudeAccount, TestEndpoints};
use crate::provider_request::{
    self, ProviderRequestInput, ProviderRequestTarget, ProviderRequests,
};
use asb_core::contracts::ProviderConnectionOptions;
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};

fn account(id: &str) -> ClaudeAccount {
    ClaudeAccount {
        id: id.into(),
        label: id.into(),
        provider: ClaudeAuthProvider::CodexOauth,
        access_token: format!("fake-managed-{id}"),
        refresh_token: None,
        expires_at_ms: Some(chrono::Utc::now().timestamp_millis() + 3_600_000),
        upstream_account_id: Some(format!("workspace-{id}")),
        github_domain: None,
    }
}

fn setup(root: &std::path::Path, base: &str) -> (ConfigStore, Arc<ClaudeAuth>) {
    let auth = ClaudeAuth::register_test_endpoints(
        root,
        TestEndpoints {
            github_api: base.into(),
            oauth_token: base.into(),
            upstream: format!("{base}/v1"),
        },
    );
    auth.save_account(account("alice"), &auth.view().unwrap().file_hash, true)
        .unwrap();
    (ConfigStore::new(root.into()), auth)
}

fn draft(base: &str) -> ProviderRequestConnection {
    serde_json::from_value(json!({"app":"claude", "baseUrl":base,"apiKey":"", "upstreamProtocol":"responses",
        "responsesOptions":{"requestMode":"standard"}, "defaultModel":"gpt-fixture", "connection":{"providerType":"codex_oauth"}})).unwrap()
}

fn completed() -> String {
    format!(
        "event: response.completed\ndata: {}\n\ndata: [DONE]\n\n",
        json!({"type":"response.completed","response":{
        "id":"r", "object":"response", "status":"completed", "model":"served-fixture", "output":[
            {"type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"managed success"}]}]}})
    )
}

#[test]
fn an_opaque_preparation_executes_with_the_backend_account_and_converts_forced_sse() {
    let dir = tempfile::tempdir().unwrap();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr());
    let (store, _) = setup(dir.path(), &base);
    let task = std::thread::spawn(move || {
        let mut request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(request.url(), "/v1/responses");
        assert_eq!(
            request
                .headers()
                .iter()
                .find(|header| header.field.equiv("authorization"))
                .unwrap()
                .value
                .as_str(),
            "Bearer fake-managed-alice"
        );
        let mut bytes = String::new();
        request.as_reader().read_to_string(&mut bytes).unwrap();
        let body: Value = serde_json::from_str(&bytes).unwrap();
        assert_eq!(body["stream"], true);
        assert_eq!(body["store"], false);
        assert!(body.get("max_output_tokens").is_none());
        request
            .respond(tiny_http::Response::from_string(completed()).with_header(
                tiny_http::Header::from_bytes("content-type", "text/event-stream").unwrap(),
            ))
            .unwrap();
    });
    let requests = ProviderRequests::default();
    let prepared = provider_request::prepare(
        &requests,
        &store,
        ProviderRequestTarget::Draft {
            connection: draft(&base),
        },
    )
    .unwrap();
    assert_eq!(prepared.endpoint, format!("{base}/v1/responses"));
    assert!(!serde_json::to_string(&prepared)
        .unwrap()
        .contains("fake-managed"));
    let outcome = tauri::async_runtime::block_on(provider_request::execute_with_client(
        requests,
        store,
        ProviderRequestInput {
            request_id: prepared.request_id,
            model: "requested-model".into(),
        },
        reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap(),
    ))
    .unwrap();
    assert_eq!(
        outcome.reply.as_deref(),
        Some("managed success"),
        "{outcome:?}"
    );
    assert_eq!(outcome.model.as_deref(), Some("served-fixture"));
    task.join().unwrap();
}

#[test]
fn a_changed_default_account_requires_a_new_preparation_instead_of_sending_to_another_account() {
    let dir = tempfile::tempdir().unwrap();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr());
    let (store, auth) = setup(dir.path(), &base);
    let requests = ProviderRequests::default();
    let prepared = provider_request::prepare(
        &requests,
        &store,
        ProviderRequestTarget::Draft {
            connection: draft(&base),
        },
    )
    .unwrap();
    auth.save_account(account("bob"), &auth.view().unwrap().file_hash, true)
        .unwrap();
    let error = tauri::async_runtime::block_on(provider_request::execute_with_client(
        requests,
        store,
        ProviderRequestInput {
            request_id: prepared.request_id,
            model: "fixture".into(),
        },
        reqwest::Client::new(),
    ))
    .unwrap_err();
    assert_eq!(error.code, "claude-account-changed");
    assert!(server
        .recv_timeout(Duration::from_millis(100))
        .unwrap()
        .is_none());
}

#[test]
fn claude_model_url_overrides_are_used_by_the_existing_opaque_model_picker() {
    let dir = tempfile::tempdir().unwrap();
    let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
    let base = format!("http://{}", server.server_addr());
    let store = ConfigStore::new(dir.path().into());
    let task = std::thread::spawn(move || {
        let request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap();
        assert_eq!(request.url(), "/custom/models");
        request
            .respond(tiny_http::Response::from_string(
                json!({"data":[{"id":"custom-model"}]}).to_string(),
            ))
            .unwrap();
    });
    let mut connection = draft(&base);
    connection.api_key = "fake-provider-key".into();
    connection.connection = ProviderConnectionOptions {
        claude_models_url: Some(format!("{base}/custom/models")),
        ..Default::default()
    };
    let requests = ProviderRequests::default();
    let prepared = provider_request::prepare(
        &requests,
        &store,
        ProviderRequestTarget::Draft { connection },
    )
    .unwrap();
    let models = tauri::async_runtime::block_on(provider_request::fetch_models(
        requests,
        store,
        prepared.request_id,
    ))
    .unwrap();
    assert_eq!(models[0].id, "custom-model");
    task.join().unwrap();
}
