use super::*;
use std::path::{Path, PathBuf};
mod wire;

#[derive(Clone, Copy)]
pub(super) enum Scenario {
    Read,
    Recover,
    Image,
}

struct StopGateway(GatewayController);
impl Drop for StopGateway {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}

#[test]
#[ignore = "requires the pinned local Claude Code binary; all configuration and upstreams are isolated"]
fn actual_claude_chat_recovers_after_a_tool_error() {
    run_case(UpstreamProtocol::ChatCompletions, Scenario::Recover);
}

#[test]
#[ignore = "requires the pinned local Claude Code binary; all configuration and upstreams are isolated"]
fn actual_claude_responses_replays_native_reasoning_with_the_tool_result() {
    run_case(UpstreamProtocol::Responses, Scenario::Read);
}

#[test]
#[ignore = "requires the pinned local Claude Code binary; all configuration and upstreams are isolated"]
fn actual_claude_native_bearer_completes_a_tool_roundtrip() {
    run_case(UpstreamProtocol::AnthropicMessages, Scenario::Read);
}

#[test]
#[ignore = "requires the pinned local Claude Code binary; all configuration and upstreams are isolated"]
fn actual_claude_chat_preserves_an_image_tool_result() {
    run_case(UpstreamProtocol::ChatCompletions, Scenario::Image);
}

fn run_case(protocol: UpstreamProtocol, scenario: Scenario) {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let upstream_url = endpoint(&upstream);
    let key = format!("fixture-cli-{}", uuid::Uuid::new_v4());
    let gateway = GatewayController::start(&local);
    let _shutdown = StopGateway(gateway.clone());
    let projection = projection(&local, &gateway, protocol, &upstream_url, &key);
    let config_file = local.target(AppKind::Claude).unwrap();
    apply(&config_file, &projection, &gateway, directory.path());
    let work = directory.path().join("work");
    let file = fixture_file(&work, scenario);
    let prompt = format!("PARITY_RUN: Read {} using Read. Recover from a failed read by reading that file. Finally reply exactly claude parity ok.", file.file_name().unwrap().to_string_lossy());
    let (stop, stopped) = mpsc::channel();
    let worker = serve(upstream, protocol, scenario, file, stopped);
    let output = super::runner::run(
        config_file.parent().unwrap(),
        &work,
        &directory.path().join("home"),
        &prompt,
        "Read",
        &upstream_url,
    );
    let _ = stop.send(());
    let requests = worker.join().unwrap();
    gateway.shutdown();
    check_result(&output, &requests, protocol, scenario, &key);
}

fn fixture_file(work: &Path, scenario: Scenario) -> PathBuf {
    fs::create_dir_all(work).unwrap();
    if matches!(scenario, Scenario::Image) {
        use base64::Engine;
        let file = work.join("fixture.png");
        let bytes = base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==").unwrap();
        fs::write(&file, bytes).unwrap();
        file
    } else {
        let file = work.join("fixture.txt");
        fs::write(&file, "PARITY_FIXTURE_OK\n").unwrap();
        file
    }
}

fn check_result(
    output: &std::process::Output,
    requests: &[Value],
    protocol: UpstreamProtocol,
    scenario: Scenario,
    key: &str,
) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw_stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = raw_stderr.replace(key, "<fixture-key>");
    assert!(output.status.success(), "Claude CLI failed: {stderr}");
    assert!(
        stdout.contains("claude parity ok"),
        "Claude CLI did not finish: {stderr}"
    );
    assert!(
        !stdout.contains(key) && !raw_stderr.contains(key),
        "CLI exposed its fixture credential"
    );
    match protocol {
        asb_core::UpstreamProtocol::GeminiGenerateContent => panic!("Google native has a separate Claude fixture"),
        UpstreamProtocol::ChatCompletions => assert_eq!(requests[0]["reasoning_effort"], "xhigh"),
        UpstreamProtocol::Responses => assert_eq!(requests[0]["reasoning"]["effort"], "xhigh"),
        UpstreamProtocol::AnthropicMessages => {
            assert_eq!(requests[0]["output_config"]["effort"], "max")
        }
    }
    let expected = if matches!(scenario, Scenario::Recover) {
        3
    } else {
        2
    };
    assert_eq!(requests.len(), expected, "unexpected number of model turns");
    assert!(
        requests[0]["tools"]
            .as_array()
            .is_some_and(|tools| tools.len() > 1),
        "fixture must exercise the full built-in tool catalogue"
    );
    if matches!(scenario, Scenario::Image) {
        let messages = requests.last().unwrap()["messages"].as_array().unwrap();
        let result = messages
            .iter()
            .find(|message| message["role"] == "tool")
            .expect("image tool result retained");
        assert_eq!(result["tool_call_id"], "call_cli_0");
        assert!(result["content"].is_string());
        assert!(messages
            .iter()
            .filter(|message| message["role"] == "user")
            .any(|message| message["content"]
                .as_array()
                .is_some_and(|parts| parts.iter().any(|part| part["type"] == "image_url"))));
    } else {
        assert!(requests
            .last()
            .unwrap()
            .to_string()
            .contains("PARITY_FIXTURE_OK"));
    }
    if protocol == UpstreamProtocol::Responses {
        let items = requests[1]["input"].as_array().unwrap();
        assert!(items.iter().any(|item| item == &wire::reasoning()));
        assert!(!requests[1].to_string().contains("asb-reasoning-"));
    }
    if matches!(scenario, Scenario::Recover) {
        assert!(requests[1].to_string().contains("[asb:tool-result-error]"));
    }
}

fn projection(
    local: &LocalState,
    gateway: &GatewayController,
    protocol: UpstreamProtocol,
    base: &str,
    key: &str,
) -> crate::gateway::GatewayProjection {
    let record = local
        .configuration()
        .create_provider(ProviderDraft {
            authentication: Some(AuthenticationScheme::Bearer),
            app: AppKind::Claude,
            route_mode: RouteMode::Custom,
            name: "CLI parity".into(),
            base_url: Some(if protocol == UpstreamProtocol::AnthropicMessages {
                base.into()
            } else {
                format!("{base}/v1")
            }),
            connection: Default::default(),
            api_key: key.into(),
            upstream_protocol: Some(protocol),
            responses_options: (protocol == UpstreamProtocol::Responses).then_some(
                asb_core::contracts::ResponsesOptions {
                    request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
                },
            ),
            max_output_tokens: None.into(),
            model: Some("gpt-5.4".into()),
            model_options: None,
            parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
            notes: None,
            website_url: None,
            usage_query: None,
            official_quota_refresh_interval_minutes: None,
        })
        .unwrap();
    crate::gateway::failover::save(
        local.root(),
        &crate::gateway::failover::ClaudeFailoverPolicy {
            takeover: true,
            ..Default::default()
        },
    )
    .unwrap();
    gateway
        .project_with_candidates(
            local,
            &SwitchPlan::direct(record.profile, default_client_settings(AppKind::Claude)),
        )
        .unwrap()
}

fn apply(
    target: &Path,
    projection: &crate::gateway::GatewayProjection,
    gateway: &GatewayController,
    root: &Path,
) {
    let backups = root.join("backups");
    let preview =
        asb_switch::read_preview(&FsIo, target, &projection.plan, &backups.to_string_lossy())
            .unwrap();
    asb_switch::execute(
        &FsIo,
        &asb_switch::SwitchRequest {
            target,
            plan: &projection.plan,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| gateway.commit(projection, || Ok(())),
    )
    .unwrap();
}

fn serve(
    upstream: Server,
    protocol: UpstreamProtocol,
    scenario: Scenario,
    file: PathBuf,
    stop: mpsc::Receiver<()>,
) -> thread::JoinHandle<Vec<Value>> {
    thread::spawn(move || {
        let mut requests = Vec::new();
        loop {
            match stop.try_recv() {
                Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
                Err(mpsc::TryRecvError::Empty) => {}
            }
            let Some(mut request) = upstream.recv_timeout(Duration::from_millis(100)).unwrap()
            else {
                continue;
            };
            if request.method() != &tiny_http::Method::Post || !request.url().starts_with('/') {
                let _ = request.respond(
                    Response::from_string("external traffic blocked by fixture")
                        .with_status_code(403),
                );
                continue;
            }
            let mut text = String::new();
            request.as_reader().read_to_string(&mut text).unwrap();
            let body: Value = serde_json::from_str(&text).unwrap();
            let main_turn = text.contains("PARITY_RUN");
            let stream = body["stream"].as_bool().unwrap_or(false);
            let step = if main_turn {
                let step = requests.len();
                requests.push(body);
                Some(step)
            } else {
                None
            };
            let response = wire::reply(protocol, scenario, step, &file, stream);
            let _ = request.respond(response);
        }
        requests
    })
}
