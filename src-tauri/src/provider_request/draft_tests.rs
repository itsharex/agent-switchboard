use super::tests::{record, TEST_KEY};
use super::*;
use asb_core::contracts::UpstreamProtocol;
use serde_json::json;
use std::net::TcpListener;

#[test]
fn draft_preparation_neither_contacts_network_nor_creates_storage() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("absent-store");
    let store = ConfigStore::new(root.clone());
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let mut connection =
        ProviderRequestConnection::try_from(&record(UpstreamProtocol::Responses).profile).unwrap();
    connection.base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let requests = ProviderRequests::default();
    let prepared = prepare(
        &requests,
        &store,
        ProviderRequestTarget::Draft { connection },
    )
    .unwrap();
    assert!(!serde_json::to_string(&prepared).unwrap().contains(TEST_KEY));
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert!(!root.exists());
    assert!(tauri::async_runtime::block_on(requests.cancel(&prepared.request_id)).unwrap());
    assert!(!root.exists());
}

#[test]
fn targets_reject_old_shapes_and_unknown_fields() {
    for value in [
        json!({"profileId":"old-input"}),
        json!({"kind":"saved", "profileId":"id", "apiKey":"injected"}),
        json!({"kind":"draft", "connection": {
            "baseUrl":"https://example.com", "apiKey":"isolated-key",
            "upstreamProtocol":"responses", "responsesOptions":{"requestMode":"standard"},
            "defaultModel":null, "payload":{"injected":true}
        }}),
    ] {
        assert!(serde_json::from_value::<ProviderRequestTarget>(value).is_err());
    }
}

#[test]
fn invalid_drafts_are_rejected_before_issuing_a_token() {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    for invalid in ["key", "protocol", "url"] {
        let mut connection =
            ProviderRequestConnection::try_from(&record(UpstreamProtocol::Responses).profile)
                .unwrap();
        match invalid {
            "key" => connection.api_key = "bad\r\nkey".into(),
            "protocol" => connection.responses_options = None,
            _ => connection.base_url = "https://username:password@example.com/v1".into(),
        }
        let error = prepare(
            &ProviderRequests::default(),
            &store,
            ProviderRequestTarget::Draft { connection },
        )
        .unwrap_err();
        let text = serde_json::to_string(&error).unwrap();
        assert!(!text.contains("password"));
        assert!(!text.contains("bad"));
    }
}
