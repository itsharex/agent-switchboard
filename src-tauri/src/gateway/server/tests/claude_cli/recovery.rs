//! The rest of the real-CLI acceptance matrix: a continuation that survives a
//! mid-turn provider change, and direct-mode recovery after gateway exit plus
//! externally edited client files. Isolated files, fake credentials, loopback.

use super::*;
use std::path::{Path, PathBuf};

pub(super) fn anthropic_text_stream(text: &str) -> String {
    let event = |kind: &str, value: Value| format!("event: {kind}\ndata: {value}\n\n");
    [
        event("message_start",json!({"type":"message_start","message":{"id":"msg_restore","type":"message","role":"assistant","model":"claude-opus-4-6","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":19,"output_tokens":0}}})),
        event("content_block_start",json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})),
        event("content_block_delta",json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":text}})),
        event("content_block_stop",json!({"type":"content_block_stop","index":0})),
        event("message_delta",json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":1}})),
        event("message_stop",json!({"type":"message_stop"})),
    ]
    .concat()
}

fn chat_tool_stream(file: &Path) -> String {
    let chunk = |delta: Value, finish: Option<&str>| json!({"id":"chat_failover_0","model":"gpt-5.4","choices":[{"index":0,"delta":delta,"finish_reason":finish}]});
    let args = json!({"file_path":file.to_string_lossy()}).to_string();
    let chunks = [
        chunk(
            json!({"role":"assistant","tool_calls":[{"index":0,"id":"call_failover_0","type":"function","function":{"name":"Read","arguments":""}}]}),
            None,
        ),
        chunk(
            json!({"tool_calls":[{"index":0,"function":{"arguments":args}}]}),
            Some("tool_calls"),
        ),
    ];
    let mut output: String = chunks
        .iter()
        .map(|chunk| format!("data: {chunk}\n\n"))
        .collect();
    output.push_str(&format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"id":"chat_failover_0","model":"gpt-5.4","choices":[],"usage":{"prompt_tokens":19,"completion_tokens":1,"total_tokens":20}})
    ));
    output
}

pub(super) fn apply_projection(
    target: &Path,
    projection: &crate::gateway::GatewayProjection,
    gateway: &GatewayController,
    root: &Path,
) {
    let backups = root.join("backups");
    let preview =
        asb_switch::read_preview(&FsIo, target, &projection.plan, &backups.to_string_lossy())
            .expect("preview isolated switch");
    let committed = projection.clone();
    asb_switch::execute(
        &FsIo,
        &asb_switch::SwitchRequest {
            target,
            plan: &projection.plan,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| gateway.commit(&committed, || Ok(())),
    )
    .expect("execute isolated switch");
}

pub(super) fn client_dirs(root: &Path) -> (PathBuf, PathBuf, PathBuf) {
    let config = root.join("claude-config");
    for path in [
        config.clone(),
        root.join("work"),
        root.join("home"),
        root.join("backups"),
    ] {
        fs::create_dir_all(&path).unwrap();
    }
    (config, root.join("work"), root.join("home"))
}

pub(super) fn stdout_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
#[ignore = "requires the pinned Claude Code CLI; only temporary files, fake credentials and loopback are used"]
fn actual_claude_continues_the_turn_on_a_fallback_provider_after_the_primary_fails() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let primary_server = Server::http(("127.0.0.1", 0)).unwrap();
    let fallback_server = Server::http(("127.0.0.1", 0)).unwrap();
    let primary_url = endpoint(&primary_server);
    let _fallback_url = endpoint(&fallback_server);
    let primary_key = format!("primary-{}", uuid::Uuid::new_v4());
    let fallback_key = format!("fallback-{}", uuid::Uuid::new_v4());
    let primary = sandbox_profile(
        &local,
        AppKind::Claude,
        "CLI 主供应商",
        endpoint(&primary_server) + "/v1",
        primary_key.clone(),
        UpstreamProtocol::ChatCompletions,
    );
    let fallback = sandbox_profile(
        &local,
        AppKind::Claude,
        "CLI 备用供应商",
        endpoint(&fallback_server),
        fallback_key.clone(),
        UpstreamProtocol::AnthropicMessages,
    );
    crate::gateway::failover::save(
        local.root(),
        &crate::gateway::failover::ClaudeFailoverPolicy {
            enabled: true,
            provider_ids: vec![primary.id.clone(), fallback.id.clone()],
            max_retries: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let gateway = GatewayController::start(&local);
    let _stop = StopGatewayLike(gateway.clone());
    let plan = SwitchPlan::direct(primary, default_client_settings(AppKind::Claude));
    let projection = gateway.project_with_candidates(&local, &plan).unwrap();
    let (config, work, home) = client_dirs(directory.path());
    apply_projection(
        &config.join("settings.json"),
        &projection,
        &gateway,
        directory.path(),
    );

    let file = work.join("failover.txt");
    fs::write(&file, "FAILOVER_FIXTURE_OK\n").unwrap();
    let prompt = format!(
        "FAILOVER_RUN: Read {} using Read. Finally reply exactly cross provider ok.",
        file.file_name().unwrap().to_string_lossy()
    );

    let (primary_stop, primary_stopped) = mpsc::channel();
    let primary_worker = {
        let file = file.clone();
        let stopped = primary_stopped;
        thread::spawn(move || {
            let mut bodies = Vec::new();
            loop {
                match stopped.try_recv() {
                    Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
                    Err(mpsc::TryRecvError::Empty) => {}
                }
                let Some(mut request) = primary_server
                    .recv_timeout(Duration::from_millis(100))
                    .unwrap()
                else {
                    continue;
                };
                let mut text = String::new();
                request.as_reader().read_to_string(&mut text).unwrap();
                let step = bodies.len();
                bodies.push(text.clone());
                let response = if text.contains("FAILOVER_RUN") && step == 0 {
                    Response::from_data(chat_tool_stream(&file).into_bytes()).with_header(
                        crate::gateway::server::tests::content_type("text/event-stream"),
                    )
                } else {
                    Response::from_string(r#"{"error":{"message":"primary unavailable"}}"#)
                        .with_status_code(tiny_http::StatusCode(503))
                };
                let _ = request.respond(response);
            }
            bodies
        })
    };
    let (fallback_stop, fallback_stopped) = mpsc::channel();
    let fallback_worker = thread::spawn(move || {
        let mut bodies = Vec::new();
        loop {
            match fallback_stopped.try_recv() {
                Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
                Err(mpsc::TryRecvError::Empty) => {}
            }
            let Some(mut request) = fallback_server
                .recv_timeout(Duration::from_millis(100))
                .unwrap()
            else {
                continue;
            };
            let mut text = String::new();
            request.as_reader().read_to_string(&mut text).unwrap();
            bodies.push(text);
            let _ = request.respond(
                Response::from_data(anthropic_text_stream("cross provider ok").into_bytes())
                    .with_header(crate::gateway::server::tests::content_type(
                        "text/event-stream",
                    )),
            );
        }
        bodies
    });

    let output = runner::run(&config, &work, &home, &prompt, "Read", &primary_url);
    let _ = primary_stop.send(());
    let _ = fallback_stop.send(());
    let primary_bodies = primary_worker.join().unwrap();
    let fallback_bodies = fallback_worker.join().unwrap();
    gateway.shutdown();

    let stdout = stdout_of(&output);
    assert!(
        output.status.success() && stdout.contains("cross provider ok"),
        "CLI must finish on the fallback provider; stdout={stdout:?}; stderr={:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        primary_bodies.len(),
        2,
        "the primary served the tool turn then failed the continuation"
    );
    assert_eq!(fallback_bodies.len(), 1, "the fallback served one turn");
    assert!(
        fallback_bodies[0].contains("FAILOVER_FIXTURE_OK"),
        "the continuation must carry the tool result to the fallback provider"
    );
    assert!(
        !stdout.contains(&primary_key) && !stdout.contains(&fallback_key),
        "no upstream credential may reach the CLI output"
    );
}

pub(super) struct StopGatewayLike(pub(super) GatewayController);
impl Drop for StopGatewayLike {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}

#[test]
#[ignore = "requires the pinned Claude Code CLI; only temporary files, fake credentials and loopback are used"]
fn actual_claude_survives_gateway_exit_and_externally_edited_client_files() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let upstream_url = endpoint(&upstream);
    let key = format!("restore-{}", uuid::Uuid::new_v4());
    let profile = sandbox_profile(
        &local,
        AppKind::Claude,
        "CLI 直连",
        upstream_url.clone(),
        key.clone(),
        UpstreamProtocol::AnthropicMessages,
    );
    let (request_sender, request_receiver) = mpsc::channel::<String>();
    let (stop, stopped) = mpsc::channel();
    let worker = thread::spawn(move || loop {
        match stopped.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let Some(mut request) = upstream.recv_timeout(Duration::from_millis(100)).unwrap() else {
            continue;
        };
        let mut text = String::new();
        request.as_reader().read_to_string(&mut text).unwrap();
        let _ = request_sender.send(text);
        let _ = request.respond(
            Response::from_data(anthropic_text_stream("recovery fixture ok").into_bytes())
                .with_header(crate::gateway::server::tests::content_type(
                    "text/event-stream",
                )),
        );
    });

    let gateway = GatewayController::start(&local);
    let _stop_gateway = StopGatewayLike(gateway.clone());
    crate::gateway::failover::save(
        local.root(),
        &crate::gateway::failover::ClaudeFailoverPolicy {
            takeover: true,
            ..Default::default()
        },
    )
    .unwrap();
    let takeover_plan =
        SwitchPlan::direct(profile.clone(), default_client_settings(AppKind::Claude));
    let taken_over = gateway
        .project_with_candidates(&local, &takeover_plan)
        .unwrap();
    let (config, work, home) = client_dirs(directory.path());
    let settings = config.join("settings.json");
    apply_projection(&settings, &taken_over, &gateway, directory.path());
    let rendered = fs::read_to_string(&settings).unwrap();
    assert!(
        rendered.contains(&gateway.configured_base_url()),
        "takeover must point the client at the loopback gateway"
    );
    assert!(
        !rendered.contains(&key),
        "the upstream key must stay app-side"
    );
    let takeover_run = runner::run_with_model(
        &config,
        &work,
        &home,
        "Return exactly: takeover ok",
        "Read",
        &upstream_url,
        Some("claude-opus-4-6"),
    );
    assert!(
        takeover_run.status.success() && stdout_of(&takeover_run).contains("recovery fixture ok"),
        "the real CLI must complete through the gateway; stdout={:?}; stderr={:?}",
        stdout_of(&takeover_run),
        String::from_utf8_lossy(&takeover_run.stderr)
    );

    // Simulated application exit: the client returns to its direct rendering.
    // Native-protocol profiles render as direct client files without the gateway.
    let direct = SwitchPlan::direct(profile.clone(), default_client_settings(AppKind::Claude));
    let backups = directory.path().join("backups");
    let preview =
        asb_switch::read_preview(&FsIo, &settings, &direct, &backups.to_string_lossy()).unwrap();
    asb_switch::execute(
        &FsIo,
        &asb_switch::SwitchRequest {
            target: &settings,
            plan: &direct,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .unwrap();
    gateway.shutdown();
    let rendered = fs::read_to_string(&settings).unwrap();
    assert!(
        rendered.contains(&upstream_url) && rendered.contains(&key),
        "direct mode must restore the upstream endpoint and its own credential"
    );
    assert!(
        !rendered.contains(&gateway.configured_base_url()),
        "direct mode must not keep pointing at the loopback gateway"
    );
    let restored_run = runner::run_with_model(
        &config,
        &work,
        &home,
        "Return exactly: restored ok",
        "Read",
        &upstream_url,
        Some("claude-opus-4-6"),
    );
    assert!(
        restored_run.status.success() && stdout_of(&restored_run).contains("recovery fixture ok"),
        "the real CLI must complete after gateway exit; stdout={:?}; stderr={:?}",
        stdout_of(&restored_run),
        String::from_utf8_lossy(&restored_run.stderr)
    );

    // An external edit adds a host-owned key; the next projection keeps it.
    let mut edited: Value = serde_json::from_str(&fs::read_to_string(&settings).unwrap()).unwrap();
    edited["hostExtra"] = json!(true);
    fs::write(&settings, serde_json::to_string(&edited).unwrap()).unwrap();
    let preview =
        asb_switch::read_preview(&FsIo, &settings, &direct, &backups.to_string_lossy()).unwrap();
    asb_switch::execute(
        &FsIo,
        &asb_switch::SwitchRequest {
            target: &settings,
            plan: &direct,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        |_| Ok(()),
    )
    .unwrap();
    let rendered = fs::read_to_string(&settings).unwrap();
    assert!(
        rendered.contains("hostExtra") && rendered.contains(&upstream_url),
        "an unclaimed host key must survive the next projection"
    );
    let recovered_run = runner::run_with_model(
        &config,
        &work,
        &home,
        "Return exactly: recovered ok",
        "Read",
        &upstream_url,
        Some("claude-opus-4-6"),
    );
    assert!(
        recovered_run.status.success()
            && stdout_of(&recovered_run).contains("recovery fixture ok"),
        "the real CLI must complete after the external change was re-projected; stdout={:?}; stderr={:?}",
        stdout_of(&recovered_run),
        String::from_utf8_lossy(&recovered_run.stderr)
    );
    let _ = stop.send(());
    let mut upstream_requests = 0;
    while request_receiver.try_recv().is_ok() {
        upstream_requests += 1;
    }
    assert_eq!(
        upstream_requests, 3,
        "takeover, restored-direct, and post-external-change runs each hit the fixture upstream"
    );
    worker.join().unwrap();
}
