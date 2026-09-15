//! Exercises the installed Claude cloud SDKs; no ASB HTTP conversion or real cloud credentials.
use super::*;
use asb_core::claude_native::{ClaudeNative, ClaudeNativeKind as Kind};
use std::{collections::BTreeMap, path::Path};
mod wire;
#[test]
#[ignore = "requires pinned Claude CLI; isolated SDK bearer fixture"]
fn actual_claude_bedrock_api_key_uses_native_sdk() {
    run(Kind::Bedrock, true);
}
#[test]
#[ignore = "requires pinned Claude CLI; isolated SDK SigV4 fixture"]
fn actual_claude_bedrock_access_keys_use_native_sigv4() {
    run(Kind::Bedrock, false);
}
#[test]
#[ignore = "requires pinned Claude CLI; isolated SDK API-key fixture"]
fn actual_claude_foundry_api_key_uses_native_sdk() {
    run(Kind::Foundry, true);
}
#[test]
#[ignore = "requires pinned Claude CLI; isolated SDK no-auth proxy fixture"]
fn actual_claude_vertex_uses_native_sdk_and_explicit_proxy_auth_bypass() {
    run(Kind::Vertex, true);
}
fn run(kind: Kind, api_key: bool) {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let server = Server::http(("127.0.0.1", 0)).unwrap();
    let url = endpoint(&server);
    let plan = plan(&local, kind, api_key, &url);
    let config = directory.path().join("config");
    let work = directory.path().join("work");
    fs::create_dir_all(&config).unwrap();
    fs::create_dir_all(&work).unwrap();
    apply(&config.join("settings.json"), &plan, directory.path());
    let (stop, stopped) = mpsc::channel();
    let worker = serve(server, kind, api_key, stopped);
    let output = runner::run(
        &config,
        &work,
        &directory.path().join("home"),
        "SDK_NATIVE_PARITY: Reply exactly native sdk ok.",
        "Read",
        &url,
    );
    let _ = stop.send(());
    let requests = worker.join().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("native sdk ok"),
        "native {kind:?}: {stdout:?}; {stderr:?}; requests={}",
        requests.len()
    );
    assert!(!stdout.contains("fake-native") && !stderr.contains("fake-native"));
    assert!(!requests.is_empty());
}
fn plan(local: &LocalState, kind: Kind, key: bool, url: &str) -> SwitchPlan {
    let environment = BTreeMap::from(
        match kind {
            Kind::Bedrock if key => vec![
                ("AWS_REGION".into(), "us-west-2".into()),
                (
                    "AWS_BEARER_TOKEN_BEDROCK".into(),
                    "fake-native-bearer".into(),
                ),
            ],
            Kind::Bedrock => vec![
                ("AWS_REGION".into(), "us-west-2".into()),
                ("AWS_ACCESS_KEY_ID".into(), "fake-native-access".into()),
                ("AWS_SECRET_ACCESS_KEY".into(), "fake-native-secret".into()),
            ],
            Kind::Foundry => vec![(
                "ANTHROPIC_FOUNDRY_API_KEY".into(),
                "fake-native-foundry".into(),
            )],
            Kind::Vertex => vec![
                (
                    "ANTHROPIC_VERTEX_PROJECT_ID".into(),
                    "fixture-project".into(),
                ),
                ("CLOUD_ML_REGION".into(), "global".into()),
                ("CLAUDE_CODE_SKIP_VERTEX_AUTH".into(), "1".into()),
            ],
        }
        .into_iter()
        .collect::<BTreeMap<String, String>>(),
    );
    let draft = ProviderDraft {
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: format!("native {kind:?}"),
        model: Some(
            if kind == Kind::Bedrock {
                "us.anthropic.claude-sonnet-4-6"
            } else {
                "claude-sonnet-4-6"
            }
            .into(),
        ),
        base_url: Some(url.into()),
        connection: asb_core::contracts::ProviderConnectionOptions {
            claude_native: Some(ClaudeNative { kind, environment }),
            ..Default::default()
        },
        api_key: String::new(),
        authentication: None,
        upstream_protocol: Some(UpstreamProtocol::AnthropicMessages),
        responses_options: None,
        max_output_tokens: None.into(),
        model_options: None,
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
        claude_fragment: Default::default(),
        notes: None,
        website_url: None,
        usage_query: None,
        display: None,
        official_quota_refresh_interval_minutes: None,
    };
    let profile = local
        .configuration()
        .create_provider(draft)
        .unwrap()
        .profile;
    SwitchPlan::direct(profile, default_client_settings(AppKind::Claude))
}
fn apply(target: &Path, plan: &SwitchPlan, root: &Path) {
    let backups = root.join("backups");
    let preview =
        asb_switch::read_preview(&FsIo, target, plan, &backups.to_string_lossy()).unwrap();
    asb_switch::execute(
        &FsIo,
        &asb_switch::SwitchRequest {
            target,
            plan,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .unwrap();
}
fn serve(
    server: Server,
    kind: Kind,
    api_key: bool,
    stop: mpsc::Receiver<()>,
) -> thread::JoinHandle<Vec<Value>> {
    thread::spawn(move || {
        let mut observed = Vec::new();
        while matches!(stop.try_recv(), Err(mpsc::TryRecvError::Empty)) {
            let Some(mut request) = server.recv_timeout(Duration::from_millis(100)).unwrap() else {
                continue;
            };
            if request.method() != &tiny_http::Method::Post || !request.url().starts_with('/') {
                let _ = request.respond(
                    Response::from_string("fixture blocks external traffic").with_status_code(403),
                );
                continue;
            }
            let mut body = String::new();
            request.as_reader().read_to_string(&mut body).unwrap();
            let value: Value = serde_json::from_str(&body).unwrap();
            let main = body.contains("SDK_NATIVE_PARITY");
            if main {
                let header = |name: &'static str| {
                    request
                        .headers()
                        .iter()
                        .find(|h| h.field.equiv(name))
                        .map(|h| h.value.as_str())
                };
                match kind {
                    Kind::Bedrock => {
                        assert!(request.url().starts_with("/model/"));
                        assert_eq!(value["anthropic_version"], "bedrock-2023-05-31");
                        let auth = header("authorization").unwrap();
                        if api_key {
                            assert_eq!(auth, "Bearer fake-native-bearer");
                        } else {
                            assert!(auth.starts_with("AWS4-HMAC-SHA256 "));
                            assert!(!auth.contains("fake-native-secret"));
                        }
                    }
                    Kind::Foundry => {
                        assert!(request.url().contains("messages"));
                        assert_eq!(
                            header("api-key").or_else(|| header("x-api-key")),
                            Some("fake-native-foundry")
                        );
                    }
                    Kind::Vertex => {
                        assert!(
                            request.url().contains("fixture-project")
                                && request.url().contains("streamRawPredict")
                        );
                    }
                }
                observed.push(value.clone());
            }
            let stream = value["stream"] == true
                || request.url().contains("stream")
                || request.url().contains("Stream");
            let _ = request.respond(wire::response(
                kind,
                stream,
                if main { "native sdk ok" } else { "probe ok" },
            ));
        }
        observed
    })
}
