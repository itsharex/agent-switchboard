//! Real Claude CLI, local Google wire fixture, isolated files and credentials.
use super::*;
use std::path::PathBuf;

struct Shutdown(GatewayController);
impl Drop for Shutdown {
    fn drop(&mut self) {
        self.0.shutdown();
    }
}

#[test]
#[ignore = "requires pinned Claude Code; only temporary files, fake credentials and loopback are used"]
fn actual_claude_gemini_replays_signed_tool_turn_without_leaking_synthetic_ids() {
    let _paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let url = endpoint(&upstream);
    let key = format!("google-fixture-{}", uuid::Uuid::new_v4());
    let profile = profile(&local, &url, &key);
    let gateway = GatewayController::start(&local);
    let _shutdown = Shutdown(gateway.clone());
    let projection = gateway
        .project(&SwitchPlan::direct(
            profile,
            default_client_settings(AppKind::Claude),
        ))
        .unwrap();
    let (config, work, home) = prepare_client(directory.path(), &projection, &gateway, &key);
    let file = work.join("fixture.txt");
    fs::write(&file, "GEMINI_FILE_READ\n").unwrap();
    let (stop, stopped) = mpsc::channel();
    let worker = serve(upstream, file, key.clone(), stopped);
    let output = runner::run_with_model(
        &config,
        &work,
        &home,
        "GEMINI_PARITY: Read fixture.txt with Read, then reply exactly gemini parity ok.",
        "Read",
        &url,
        Some("claude-opus-4-6"),
    );
    let _ = stop.send(());
    let requests = worker.join().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "isolated CLI: {} {}",
        stdout.replace(&key, "<key>"),
        stderr.replace(&key, "<key>")
    );
    assert!(
        stdout.contains("gemini parity ok"),
        "stdout={stdout:?}; stderr={stderr:?}; request count={}; last contents={}",
        requests.len(),
        requests
            .last()
            .map(|r| r["contents"].to_string())
            .unwrap_or_default()
    );
    assert!(!stdout.contains(&key) && !stderr.contains(&key));
    assert!(!stdout.contains("private Google thought"));
    assert_eq!(requests.len(), 2, "one tool turn then one reply");
    assert_eq!(
        requests[0]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "high"
    );
    let parts = requests[1]["contents"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["parts"].as_array().unwrap())
        .collect::<Vec<_>>();
    assert!(parts
        .iter()
        .any(|p| p.get("thoughtSignature") == Some(&json!("google-tool-signature"))));
    assert!(
        parts
            .iter()
            .any(|p| p.get("functionResponse").is_some()
                && p.to_string().contains("GEMINI_FILE_READ"))
    );
    assert!(!requests[1].to_string().contains("asb_gemini_"));
    assert!(!requests[1].to_string().contains("asb-reasoning-"));
}

fn profile(local: &LocalState, url: &str, key: &str) -> asb_core::ProviderProfile {
    let profile = sandbox_profile(
        local,
        AppKind::Claude,
        "Google Claude fixture",
        url.into(),
        key.into(),
        UpstreamProtocol::GeminiGenerateContent,
    );
    let record = local
        .configuration()
        .find_provider_record(&profile.id)
        .unwrap();
    let mut draft = serde_json::to_value(&profile).unwrap();
    draft.as_object_mut().unwrap().remove("id");
    draft["model"] = json!("gemini-3.1-flash");
    local
        .configuration()
        .update_provider(
            &profile.id,
            serde_json::from_value(draft).unwrap(),
            &record.file_hash,
        )
        .unwrap()
        .profile
}

fn serve(
    server: Server,
    file: PathBuf,
    key: String,
    stop: mpsc::Receiver<()>,
) -> thread::JoinHandle<Vec<Value>> {
    thread::spawn(move || {
        let mut requests = Vec::new();
        while matches!(stop.try_recv(), Err(mpsc::TryRecvError::Empty)) {
            let Some(mut request) = server.recv_timeout(Duration::from_millis(100)).unwrap() else {
                continue;
            };
            if request.method() != &tiny_http::Method::Post
                || !request.url().starts_with("/v1/models/")
            {
                let _ = request.respond(
                    Response::from_string("fixture blocks external traffic").with_status_code(403),
                );
                continue;
            }
            assert_eq!(
                request
                    .headers()
                    .iter()
                    .find(|h| h.field.equiv("x-goog-api-key"))
                    .map(|h| h.value.as_str()),
                Some(key.as_str())
            );
            assert!(!request
                .headers()
                .iter()
                .any(|h| h.field.equiv("authorization")
                    || h.field.equiv("x-api-key")
                    || h.field.equiv("anthropic-beta")));
            let stream = request.url().contains(":streamGenerateContent?alt=sse");
            assert!(!request.url().contains("beta=true"));
            let mut text = String::new();
            request.as_reader().read_to_string(&mut text).unwrap();
            let body: Value = serde_json::from_str(&text).unwrap();
            assert!(body.get("model").is_none() && body.get("stream").is_none());
            let main = text.contains("GEMINI_PARITY");
            let tool = main && requests.is_empty();
            if main {
                requests.push(body);
            }
            let parts = if tool {
                json!([
                    {"thought":true,"text":"private Google thought","thoughtSignature":"google-thought-signature"},
                    {"functionCall":{"name":"Read","args":{"file_path":file.to_string_lossy()}},"thoughtSignature":"google-tool-signature"}
                ])
            } else {
                json!([{"text":if main {"gemini parity ok"} else {"probe ok"}}])
            };
            let response = json!({"responseId":format!("google_{}",requests.len()),"modelVersion":"gemini-3.1-flash","candidates":[{"index":0,"content":{"role":"model","parts":parts},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":24,"candidatesTokenCount":3,"thoughtsTokenCount":2}});
            let body = if stream {
                format!("data: {response}\n\n")
            } else {
                response.to_string()
            };
            let _ = request.respond(Response::from_string(body).with_header(content_type(
                if stream {
                    "text/event-stream"
                } else {
                    "application/json"
                },
            )));
        }
        requests
    })
}
