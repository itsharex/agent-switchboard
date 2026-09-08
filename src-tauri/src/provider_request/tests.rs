use super::*;
use asb_core::contracts::{
    AppKind, ProviderDraft, ProviderProfile, ProviderRecord, RouteMode, UpstreamProtocol,
};
use serde_json::{json, Value};
use std::net::TcpListener;
use std::time::Duration;

pub(super) const TEST_KEY: &str = "isolated-verification-secret";

pub(super) fn draft(protocol: UpstreamProtocol, base: &str) -> ProviderDraft {
    ProviderDraft {
        app: AppKind::Codex,
        route_mode: RouteMode::Custom,
        name: "Isolated provider".to_string(),
        model: Some("configured-model".to_string()),
        base_url: Some(base.to_string()),
        api_key: TEST_KEY.to_string(),
        upstream_protocol: Some(protocol),
        responses_options: (Some(protocol)
            == Some(asb_core::contracts::UpstreamProtocol::Responses))
        .then_some(asb_core::contracts::ResponsesOptions {
            request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
        }),
        max_output_tokens: if protocol == UpstreamProtocol::AnthropicMessages {
            Some(1_024).into()
        } else {
            None.into()
        },
        model_options: None,
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

pub(super) fn record(protocol: UpstreamProtocol) -> ProviderRecord {
    ProviderRecord {
        profile: ProviderProfile::from_draft(
            uuid::Uuid::new_v4().to_string(),
            draft(protocol, "http://127.0.0.1:1/v1"),
        ),
        file_hash: "isolated-revision".to_string(),
    }
}

pub(super) fn success_body(protocol: UpstreamProtocol, text: &str) -> Value {
    match protocol {
        UpstreamProtocol::Responses => json!({
            "object": "response", "status": "completed", "model": "actual-model",
            "output": [{"type":"message", "role":"assistant", "status":"completed",
                "content":[{"type":"output_text", "text":text}]}],
        }),
        UpstreamProtocol::ChatCompletions => json!({
            "object":"chat.completion", "model":"actual-model", "choices":[{
                "finish_reason":"stop", "message":{"role":"assistant", "content":text}}],
        }),
        UpstreamProtocol::AnthropicMessages => json!({
            "type":"message", "role":"assistant", "stop_reason":"end_turn",
            "model":"actual-model", "content":[{"type":"text", "text":text}],
        }),
    }
}

pub(super) fn direct_client() -> reqwest::Client {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(3))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .unwrap()
}

#[test]
fn preparation_is_credential_free_and_does_not_contact_the_provider() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let record = store
        .create_provider(draft(UpstreamProtocol::Responses, &base))
        .unwrap();
    let preparation = prepare(
        &ProviderRequests::default(),
        &store,
        ProviderRequestTarget::Saved {
            profile_id: record.profile.id.clone(),
        },
    )
    .unwrap();
    assert_eq!(preparation.endpoint, format!("{base}/responses"));
    assert_eq!(
        preparation.default_model.as_deref(),
        Some("configured-model")
    );
    assert_eq!(preparation.prompt, "请只回复：连接成功。");
    assert!(!serde_json::to_string(&preparation)
        .unwrap()
        .contains(TEST_KEY));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        store
            .find_provider_record(&record.profile.id)
            .unwrap()
            .file_hash,
        record.file_hash
    );
}

#[test]
fn provider_storage_rejects_sensitive_url_components_before_preparation() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    for base in [
        format!("http://request-user-secret:request-password-secret@{address}/v1"),
        format!("http://{address}/v1?token=request-query-secret"),
        format!("http://{address}/v1#request-fragment-secret"),
        format!("http://{address}/\nrequest-control-secret"),
    ] {
        let error = store
            .create_provider(draft(UpstreamProtocol::Responses, &base))
            .unwrap_err();
        let serialized = error.to_string();
        for secret in [
            TEST_KEY,
            "request-user-secret",
            "request-password-secret",
            "request-query-secret",
            "request-fragment-secret",
            "request-control-secret",
        ] {
            assert!(!serialized.contains(secret));
        }
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn official_profiles_cannot_prepare_a_request() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    let mut official = draft(UpstreamProtocol::Responses, "http://127.0.0.1:1");
    official.route_mode = RouteMode::Official;
    official.base_url = None;
    official.api_key.clear();
    official.model = None;
    official.upstream_protocol = None;
    official.responses_options = None;
    let record = store.create_provider(official).unwrap();
    assert_eq!(
        prepare(
            &ProviderRequests::default(),
            &store,
            ProviderRequestTarget::Saved {
                profile_id: record.profile.id.clone()
            }
        )
        .unwrap_err()
        .code,
        "provider-request-custom-only"
    );
}

#[test]
fn changed_profile_is_rejected_before_requests_without_network() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("state");
    let store = ConfigStore::new(path.clone());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}/v1", listener.local_addr().unwrap());
    let record = store
        .create_provider(draft(UpstreamProtocol::Responses, &base))
        .unwrap();
    let requests = ProviderRequests::default();
    let preparation = prepare(
        &requests,
        &store,
        ProviderRequestTarget::Saved {
            profile_id: record.profile.id.clone(),
        },
    )
    .unwrap();
    let mut changed = draft(UpstreamProtocol::Responses, &base);
    changed.api_key = "changed-isolated-key".to_string();
    store
        .update_provider(&record.profile.id, changed, &record.file_hash)
        .unwrap();
    let listing = tauri::async_runtime::block_on(fetch_models(
        requests.clone(),
        ConfigStore::new(path.clone()),
        preparation.request_id.clone(),
    ))
    .unwrap_err();
    assert_eq!(listing.code, "provider-request-profile-changed");
    // Listing does not consume a live token, but neither operation may switch
    // to the profile's new connection behind the prepared target's back.
    assert!(requests.inspect(&preparation.request_id).is_ok());
    let input = ProviderRequestInput {
        request_id: preparation.request_id.clone(),
        model: "test-model".to_string(),
    };
    let error = tauri::async_runtime::block_on(execute_with_client(
        requests.clone(),
        ConfigStore::new(path),
        input,
        direct_client(),
    ))
    .unwrap_err();
    assert_eq!(error.code, "provider-request-profile-changed");
    assert!(requests.inspect(&preparation.request_id).is_err());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn request_contract_rejects_renderer_supplied_credentials_and_payload() {
    for extra in ["apiKey", "endpoint", "prompt", "upstreamProtocol"] {
        let mut value = json!({"requestId":"id", "model":"model"});
        value[extra] = json!("untrusted");
        assert!(serde_json::from_value::<ProviderRequestInput>(value).is_err());
    }
}

#[test]
fn models_fetch_accepts_only_a_live_preparation_token() {
    let directory = tempfile::tempdir().unwrap();
    let requests = ProviderRequests::default();
    let error = tauri::async_runtime::block_on(fetch_models(
        requests,
        ConfigStore::new(directory.path().join("state")),
        "unknown-token".to_string(),
    ))
    .unwrap_err();
    assert_eq!(error.code, "provider-request-unavailable");
}
