use super::*;

#[test]
#[ignore = "requires an installed Claude Code CLI and an isolated loopback sandbox"]
fn actual_claude_code_completes_through_the_isolated_gateway() {
    Command::new("claude.cmd")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("the explicit black-box test requires the Claude Code CLI");

    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = endpoint(&upstream);
    let upstream_key = format!("fixture-{}", uuid::Uuid::new_v4());
    let (observed_sender, observed_receiver) = mpsc::channel();
    let upstream_worker = serve_cli_upstream(upstream, observed_sender);

    let directory = tempfile::tempdir().expect("temporary sandbox root");
    let state = LocalState::from_root(directory.path().join("state"));
    let profile = sandbox_profile(
        &state,
        AppKind::Claude,
        "Claude Code CLI black-box sandbox",
        upstream_url,
        upstream_key.clone(),
        UpstreamProtocol::ChatCompletions,
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project(&SwitchPlan::direct(
            profile,
            default_client_settings(AppKind::Claude),
        ))
        .expect("project route");

    let (claude_config, workdir, home) =
        prepare_client(directory.path(), &projection, &gateway, &upstream_key);

    let output = run_isolated_cli(&claude_config, workdir, &home);
    assert_cli_output(&output, &upstream_key, &projection_token(&projection));

    let (path, authorization, body) = observed_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("one actual upstream request");
    assert_eq!(path, "/v1/chat/completions");
    assert_eq!(authorization, Some(format!("Bearer {upstream_key}")));
    assert!(!body.contains(&projection_token(&projection)));
    assert!(body.contains("Return exactly: sandbox answer"));
    let upstream_body: Value = serde_json::from_str(&body).expect("Chat upstream JSON");
    assert_eq!(upstream_body["reasoning_effort"], "max");
    upstream_worker.join().expect("upstream worker");
    gateway.shutdown();
}

fn serve_cli_upstream(
    upstream: Server,
    observed_sender: mpsc::Sender<(String, Option<String>, String)>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(30))
            .expect("upstream receive")
            .expect("upstream request");
        let path = request.url().to_string();
        let authorization = request
            .headers()
            .iter()
            .find(|header| header.field.equiv("Authorization"))
            .map(|header| header.value.as_str().to_string());
        let mut body = String::new();
        request
            .as_reader()
            .read_to_string(&mut body)
            .expect("read upstream request");
        let stream_requested = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|value| value.get("stream").and_then(Value::as_bool))
            .unwrap_or(false);
        observed_sender
            .send((path, authorization, body))
            .expect("report upstream request");
        let response = if stream_requested {
            let stream = b"data: {\"id\":\"chat_claude\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_claude\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"private sandbox reasoning\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_claude\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"sandbox answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_claude\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
            Response::from_data(stream.as_slice())
                .with_header(content_type("text/event-stream; charset=utf-8"))
        } else {
            Response::from_data(
                serde_json::to_vec(&json!({
                    "id": "chat_claude",
                    "object": "chat.completion",
                    "model": "sandbox-model",
                    "choices": [{
                        "index": 0,
                        "message": {
                            "role": "assistant",
                            "reasoning_content": "private sandbox reasoning",
                            "content": "sandbox answer"
                        },
                        "finish_reason": "stop",
                        "logprobs": null
                    }],
                    "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
                }))
                .expect("Chat response JSON"),
            )
            .with_header(content_type("application/json"))
        };
        request.respond(response).expect("respond upstream");
    })
}

fn prepare_client(
    root: &std::path::Path,
    projection: &crate::gateway::GatewayProjection,
    gateway: &GatewayController,
    upstream_key: &str,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let claude_config = root.join("claude-config");
    let workdir = root.join("work");
    let home = root.join("home");
    fs::create_dir_all(&claude_config).expect("create temporary Claude config");
    fs::create_dir_all(&workdir).expect("create temporary working directory");
    fs::create_dir_all(&home).expect("create temporary home");
    let settings = claude_config.join("settings.json");
    let backup_dir = root.join("backups");
    let io = FsIo;
    let preview = asb_switch::read_preview(
        &io,
        &settings,
        &projection.plan,
        &backup_dir.to_string_lossy(),
    )
    .expect("preview isolated Claude switch");
    let committed_projection = projection.clone();
    let committed_gateway = gateway.clone();
    asb_switch::execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &settings,
            plan: &projection.plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        move |_| committed_gateway.commit(&committed_projection, || Ok(())),
    )
    .expect("execute isolated Claude switch");
    let rendered = fs::read_to_string(&settings).expect("read isolated Claude settings");
    assert!(rendered.contains("ANTHROPIC_BASE_URL"));
    assert!(!rendered.contains(upstream_key));
    let rendered_settings: Value =
        serde_json::from_str(&rendered).expect("isolated Claude settings JSON");
    assert_eq!(
        rendered_settings["env"]["CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS"],
        "1"
    );

    (claude_config, workdir, home)
}

fn run_isolated_cli(
    claude_config: &std::path::Path,
    workdir: std::path::PathBuf,
    home: &std::path::Path,
) -> std::process::Output {
    let mut child = Command::new("claude.cmd")
        .args([
            "-p",
            "--no-session-persistence",
            "--effort",
            "max",
            "--setting-sources",
            "user",
            "Return exactly: sandbox answer",
        ])
        .current_dir(workdir)
        .env("CLAUDE_CONFIG_DIR", &claude_config)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("APPDATA", home.join("AppData"))
        .env("LOCALAPPDATA", home.join("LocalAppData"))
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_AUTH_TOKEN")
        .env_remove("ANTHROPIC_BASE_URL")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start isolated Claude Code");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if child.try_wait().expect("poll Claude Code").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("isolated Claude Code did not finish within 60 seconds");
        }
        thread::sleep(Duration::from_millis(50));
    }
    child
        .wait_with_output()
        .expect("collect Claude Code output")
}

fn assert_cli_output(output: &std::process::Output, upstream_key: &str, loopback_token: &str) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let safe_stdout = stdout
        .replace(upstream_key, "<redacted-upstream-key>")
        .replace(loopback_token, "<redacted-loopback-token>");
    let safe_stderr = stderr
        .replace(upstream_key, "<redacted-upstream-key>")
        .replace(loopback_token, "<redacted-loopback-token>");
    assert!(
        output.status.success(),
        "isolated Claude Code must succeed; stdout={safe_stdout:?}; stderr={safe_stderr:?}"
    );
    assert!(stdout.contains("sandbox answer"));
    assert!(!stdout.contains("private sandbox reasoning"));
    assert!(!stdout.contains(upstream_key));
    assert!(!stderr.contains(upstream_key));
}
