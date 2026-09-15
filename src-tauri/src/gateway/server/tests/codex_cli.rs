use super::*;
use asb_core::contracts::ResponsesRequestMode;

#[test]
#[ignore = "requires an installed Codex CLI and an isolated loopback sandbox"]
fn actual_codex_cli_completes_through_the_isolated_http_gateway() {
    Command::new(codex_executable())
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("the explicit black-box test requires the Codex CLI");

    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let upstream_url = endpoint(&upstream);
    let upstream_key = format!("fixture-{}", uuid::Uuid::new_v4());
    let (observed_sender, observed_receiver) = mpsc::channel();
    let upstream_worker =
        serve_cli_upstream(upstream, observed_sender, UpstreamProtocol::ChatCompletions);

    let directory = tempfile::tempdir().expect("temporary sandbox root");
    let state = LocalState::from_root(directory.path().join("state"));
    let mut file = sandbox_codex_file(
        &state,
        "Codex CLI black-box sandbox",
        upstream_url,
        upstream_key.clone(),
        CodexUpstream::ChatCompletions,
    );
    let gateway = GatewayController::start(&state);
    file.parameters.settings.insert(
        "web_search".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Str("live".to_string()),
        },
    );
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .expect("project route");

    let (codex_home, workdir) =
        prepare_client(directory.path(), &projection, &gateway, &upstream_key);

    let output = run_isolated_cli(&codex_home, workdir);
    assert_cli_output(&output, &upstream_key, &projection_token(&projection));
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("Falling back from WebSockets"),
        "Codex CLI must complete through the local WebSocket gateway"
    );

    let (authorization, body) = observed_receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("one actual upstream request after local prewarm");
    assert_eq!(authorization, Some(format!("Bearer {upstream_key}")));
    assert!(!body.contains(&projection_token(&projection)));
    assert!(!body.contains("previous_response_id"));
    assert!(body.contains("Return exactly: sandbox answer"));
    let upstream_body: Value = serde_json::from_str(&body).expect("Chat upstream JSON");
    let tools = upstream_body["tools"]
        .as_array()
        .expect("actual Codex request must include tools");
    assert!(tools.iter().all(|tool| tool["type"] == "function"));
    assert!(tools.iter().any(|tool| {
        tool["function"]["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("asbns_n_"))
    }));
    upstream_worker.join().expect("upstream worker");
    gateway.shutdown();
}

#[test]
#[ignore = "requires an installed Codex CLI and an isolated loopback sandbox"]
fn actual_codex_cli_preserves_official_login_through_native_responses_gateway() {
    verify_responses_cli(ResponsesRequestMode::Standard, false);
}

#[test]
#[ignore = "requires an installed Codex CLI and an isolated loopback sandbox"]
fn actual_codex_cli_completes_through_minimal_responses() {
    verify_responses_cli(ResponsesRequestMode::Minimal, false);
}

#[test]
#[ignore = "requires the repository Codex CLI and an isolated loopback sandbox"]
fn actual_codex_cli_uses_gateway_without_chatgpt_login() {
    verify_responses_cli(ResponsesRequestMode::Standard, true);
}

fn verify_responses_cli(mode: ResponsesRequestMode, api_key_only: bool) {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let upstream_url = endpoint(&upstream);
    let upstream_key = format!("fixture-{}", uuid::Uuid::new_v4());
    let (sender, receiver) = mpsc::channel();
    let worker = serve_cli_upstream(upstream, sender, UpstreamProtocol::Responses);
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let mut file = sandbox_codex_file(
        &state,
        "Native HTTP sandbox",
        upstream_url,
        upstream_key.clone(),
        CodexUpstream::Responses,
    );
    file.profile.request_mode = mode;
    file.parameters.settings.insert(
        "web_search".to_string(),
        SettingValue::Explicit {
            value: ConfigValue::Str("disabled".to_string()),
        },
    );
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project_codex(&file, default_client_settings(AppKind::Codex))
        .unwrap();
    assert!(projection.plan.profile.requires_gateway());
    let (codex_home, workdir) =
        prepare_client(directory.path(), &projection, &gateway, &upstream_key);
    if api_key_only {
        fs::write(
            codex_home.join("auth.json"),
            serde_json::json!({
                "auth_mode": "apikey", "OPENAI_API_KEY": "asb-local-gateway"
            })
            .to_string(),
        )
        .unwrap();
    }
    let output = run_isolated_cli(&codex_home, workdir);
    assert_cli_output(&output, &upstream_key, &projection_token(&projection));
    let (authorization, body) = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(authorization, Some(format!("Bearer {upstream_key}")));
    let request: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(request["model"], "sandbox-model");
    assert_eq!(request["stream"], true);
    assert!(body.contains("Return exactly: sandbox answer"));
    if mode == ResponsesRequestMode::Minimal {
        for field in [
            "reasoning",
            "service_tier",
            "store",
            "include",
            "metadata",
            "client_metadata",
            "prompt_cache_key",
        ] {
            assert!(
                request.get(field).is_none(),
                "{field} must not reach the upstream"
            );
        }
    }
    worker.join().unwrap();
    gateway.shutdown();
}

fn serve_cli_upstream(
    upstream: Server,
    observed_sender: mpsc::Sender<(Option<String>, String)>,
    protocol: UpstreamProtocol,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(30))
            .expect("upstream receive")
            .expect("upstream request");
        assert_eq!(
            request.method(),
            &Method::Post,
            "the first request must use HTTP POST"
        );
        assert!(
            !request
                .headers()
                .iter()
                .any(|header| header.field.equiv("Upgrade")),
            "WebSocket must not be attempted"
        );
        assert_eq!(
            request.url(),
            if protocol == UpstreamProtocol::Responses {
                "/v1/responses"
            } else {
                "/v1/chat/completions"
            }
        );
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
        observed_sender
            .send((authorization, body))
            .expect("report upstream request");
        let stream = b"data: {\"id\":\"chat_cli\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_cli\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"reasoning_content\":\"private sandbox reasoning\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_cli\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"sandbox answer\"},\"finish_reason\":null}]}\n\ndata: {\"id\":\"chat_cli\",\"object\":\"chat.completion.chunk\",\"created\":0,\"model\":\"sandbox-model\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        request
            .respond(
                Response::from_data(cli_stream_body(protocol, stream))
                    .with_header(content_type("text/event-stream; charset=utf-8")),
            )
            .expect("respond upstream");
    })
}

fn cli_stream_body(protocol: UpstreamProtocol, stream: &[u8]) -> Vec<u8> {
    if protocol != UpstreamProtocol::Responses {
        return stream.to_vec();
    }
    let transport = ReasoningTransport::from_continuation_key([9; 32]);
    let mut transcoder = crate::gateway::transform::SseTranscoder::new(
        stream,
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::Responses,
        8192,
        Some(&transport),
    )
    .unwrap();
    let mut body = Vec::new();
    transcoder.read_to_end(&mut body).unwrap();
    body
}

pub(super) fn prepare_client(
    root: &std::path::Path,
    projection: &crate::gateway::GatewayProjection,
    gateway: &GatewayController,
    upstream_key: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let codex_home = root.join("codex-home");
    let workdir = root.join("work");
    fs::create_dir_all(&codex_home).expect("create temporary Codex home");
    fs::create_dir_all(&workdir).expect("create temporary working directory");
    let config = codex_home.join("config.toml");
    let auth = codex_home.join("auth.json");
    let backup_dir = root.join("backups");
    let io = FsIo;
    let fixture_auth = fake_official_auth();
    fs::write(&auth, &fixture_auth).unwrap();
    fs::write(&config, "cli_auth_credentials_store = \"file\"\nchatgpt_base_url = \"http://127.0.0.1:9/isolated\"\n").unwrap();
    let catalog = projection
        .codex_catalog
        .as_ref()
        .expect("Codex projection must carry its catalog artifact");
    let catalog_path = codex_home.join(&catalog.file_name);
    fs::write(&catalog_path, &catalog.content).expect("write isolated model catalog");
    let preview = asb_switch::read_preview(
        &io,
        &config,
        &projection.plan,
        &backup_dir.to_string_lossy(),
    )
    .expect("preview isolated Codex switch");
    let committed_projection = projection.clone();
    let committed_gateway = gateway.clone();
    asb_switch::execute(
        &io,
        &asb_switch::SwitchRequest {
            target: &config,
            plan: &projection.plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        move |_| committed_gateway.commit(&committed_projection, || Ok(())),
    )
    .expect("execute isolated Codex switch");
    let rendered = fs::read_to_string(&config).expect("read isolated Codex config");
    assert!(rendered.contains("web_search = \"disabled\""));
    assert!(!rendered.contains(upstream_key));
    assert!(rendered.contains("model_provider = \"openai\""));
    assert!(!rendered.contains("model_providers"));
    assert!(!rendered.contains("experimental_bearer_token"));
    assert_eq!(fs::read(&auth).unwrap(), fixture_auth);
    assert_eq!(fs::read_to_string(catalog_path).unwrap(), catalog.content);

    (codex_home, workdir)
}

fn run_isolated_cli(
    codex_home: &std::path::Path,
    workdir: std::path::PathBuf,
) -> std::process::Output {
    let auth_before = fs::read(codex_home.join("auth.json")).unwrap();
    let mut command = isolated_codex(codex_home);
    let mut child = command
        .args([
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "Return exactly: sandbox answer",
        ])
        .current_dir(workdir)
        .env("CODEX_HOME", &codex_home)
        .env_remove("OPENAI_API_KEY")
        .env_remove("OPENAI_BASE_URL")
        .env_remove("OPENAI_API_BASE")
        .env_remove("CODEX_API_KEY")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start isolated Codex CLI");
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if child.try_wait().expect("poll Codex CLI").is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("isolated Codex CLI did not finish within 45 seconds");
        }
        thread::sleep(Duration::from_millis(50));
    }
    let output = child.wait_with_output().expect("collect Codex CLI output");
    assert_eq!(fs::read(codex_home.join("auth.json")).unwrap(), auth_before);
    output
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
        "isolated Codex CLI must succeed; stdout={safe_stdout:?}; stderr={safe_stderr:?}"
    );
    assert!(stdout.contains("sandbox answer"));
    assert!(!stdout.contains("private sandbox reasoning"));
    assert!(!stdout.contains(upstream_key));
    assert!(!stderr.contains(upstream_key));
}

fn codex_executable() -> std::path::PathBuf {
    std::env::var_os("ASB_TEST_CODEX_BIN")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| "codex".into())
}

pub(super) fn isolated_codex(codex_home: &std::path::Path) -> Command {
    let mut command = Command::new(codex_executable());
    command.env_clear();
    for name in [
        "PATH",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "PATHEXT",
        "TEMP",
        "TMP",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for name in ["HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"] {
        command.env(name, codex_home.parent().unwrap());
    }
    command
        .env("CODEX_HOME", codex_home)
        .env("NO_PROXY", "127.0.0.1,localhost");
    command
}

fn fake_official_auth() -> Vec<u8> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let claims = json!({"exp":chrono::Utc::now().timestamp()+86400,"email":"fixture@example.invalid",
        "https://api.openai.com/auth":{"chatgpt_account_id":"fixture-account","chatgpt_plan_type":"plus"}});
    let jwt = format!(
        "{}.{}.fixture",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"none"}"#),
        URL_SAFE_NO_PAD.encode(claims.to_string())
    );
    json!({"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"id_token":jwt,
        "access_token":"fixture-official-access","refresh_token":"fixture-refresh","account_id":"fixture-account"},
        "last_refresh":chrono::Utc::now().to_rfc3339()}).to_string().into_bytes()
}
