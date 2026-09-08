use super::tests::{fixture, http};
use crate::config_store::ConfigStore;
use crate::provider_request::tests::{direct_client, record, success_body, TEST_KEY};
use crate::provider_request::{
    execute_with_client, prepare, ProviderRequestConnection, ProviderRequestInput,
    ProviderRequestTarget, ProviderRequests,
};
use asb_core::contracts::UpstreamProtocol;

#[test]
fn all_draft_protocols_execute_once_using_unsaved_connection_without_creating_files() {
    for protocol in [
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::AnthropicMessages,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("absent-store");
        let store = ConfigStore::new(root.clone());
        let (url, server) = fixture(http(
            200,
            &success_body(protocol, "draft reply").to_string(),
        ));
        let mut connection =
            ProviderRequestConnection::try_from(&record(protocol).profile).unwrap();
        connection.base_url = url.trim_end_matches("/test").to_string();
        connection.default_model = Some("draft-model".into());
        let requests = ProviderRequests::default();
        let prepared = prepare(
            &requests,
            &store,
            ProviderRequestTarget::Draft { connection },
        )
        .unwrap();
        assert_eq!(prepared.default_model.as_deref(), Some("draft-model"));
        assert!(!root.exists());
        let input = || ProviderRequestInput {
            request_id: prepared.request_id.clone(),
            model: "draft-model".into(),
        };
        let result = tauri::async_runtime::block_on(execute_with_client(
            requests.clone(),
            ConfigStore::new(root.clone()),
            input(),
            direct_client(),
        ))
        .unwrap();
        assert_eq!(result.reply.as_deref(), Some("draft reply"));
        let raw = server.join().unwrap();
        assert!(raw.contains(TEST_KEY));
        assert!(raw.contains("draft-model"));
        let path = reqwest::Url::parse(&prepared.endpoint).unwrap();
        assert!(raw.starts_with(&format!("POST {} HTTP/1.1", path.path())));
        assert!(tauri::async_runtime::block_on(execute_with_client(
            requests.clone(),
            store,
            input(),
            direct_client()
        ))
        .is_err());
        assert!(!root.exists());
    }
}
