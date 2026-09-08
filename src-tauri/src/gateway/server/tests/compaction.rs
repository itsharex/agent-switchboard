use super::*;

#[test]
fn legacy_and_v2_compaction_continue_through_each_protocol_without_exposing_payloads() {
    for protocol in [
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::AnthropicMessages,
    ] {
        run(protocol, false);
    }
    run(UpstreamProtocol::Responses, true);
}

fn run(protocol: UpstreamProtocol, minimal: bool) {
    let upstream = Server::http(("127.0.0.1", 0)).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let state = LocalState::from_root(directory.path().join("state"));
    let mut profile = sandbox_profile(
        &state,
        AppKind::Codex,
        "compact",
        endpoint(&upstream),
        "vendor-key".into(),
        protocol,
    );
    if minimal {
        profile.responses_options.as_mut().unwrap().request_mode =
            asb_core::contracts::ResponsesRequestMode::Minimal;
    }
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project(&SwitchPlan::direct(
            profile,
            default_client_settings(AppKind::Codex),
        ))
        .unwrap();
    gateway.commit(&projection, || Ok(())).unwrap();
    let worker = thread::spawn(move || summary_upstream(upstream, protocol));
    let client = Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let initial = json!({"model":"sandbox-model","input":[{"type":"message","role":"user","content":"Fix parser; tests remain."}]});
    let legacy = client
        .post(format!("{}/compact", codex_endpoint(&projection)))
        .bearer_auth("fixture-official-access")
        .body(initial.to_string())
        .send()
        .unwrap();
    assert_eq!(legacy.status().as_u16(), 200);
    let legacy: Value = serde_json::from_str(&legacy.text().unwrap()).unwrap();
    assert_eq!(legacy["output"][0]["type"], "compaction");
    let mut input = legacy["output"].as_array().unwrap().clone();
    input.push(json!({"type":"compaction_trigger"}));
    let v2 = client
        .post(codex_endpoint(&projection))
        .body(json!({"model":"sandbox-model","input":input,"stream":true}).to_string())
        .send()
        .unwrap();
    assert_eq!(v2.status().as_u16(), 200);
    let events = v2.text().unwrap();
    assert_eq!(
        events.matches("event: response.output_item.done\n").count(),
        1
    );
    assert!(events.contains("event: response.completed"));
    assert!(!events.contains("Parser fixed; tests remain."));
    worker.join().unwrap();
    gateway.shutdown();
}

fn summary_upstream(upstream: Server, protocol: UpstreamProtocol) {
    for index in 0..2 {
        let mut request = upstream
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .unwrap();
        assert_eq!(request.method(), &Method::Post);
        assert!(!request.headers().iter().any(
            |header| header.field.equiv("Upgrade") || header.field.equiv("ChatGPT-Account-ID")
        ));
        let mut text = String::new();
        request.as_reader().read_to_string(&mut text).unwrap();
        assert!(!text.contains("asb-compaction-v1."));
        assert!(!text.contains("fixture-official-access"));
        if index == 1 {
            assert!(text.contains("Parser fixed; tests remain."));
        }
        let body: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["stream"], false);
        assert!(body
            .get("tools")
            .is_none_or(|tools| tools.as_array().is_some_and(Vec::is_empty)));
        let response = match protocol {
            UpstreamProtocol::Responses => {
                json!({"id":"summary","object":"response","model":"sandbox-model","status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Parser fixed; tests remain."}]}],"error":null})
            }
            UpstreamProtocol::ChatCompletions => {
                json!({"id":"summary","object":"chat.completion","model":"sandbox-model","choices":[{"index":0,"message":{"role":"assistant","content":"Parser fixed; tests remain."},"finish_reason":"stop"}]})
            }
            UpstreamProtocol::AnthropicMessages => {
                json!({"id":"summary","type":"message","role":"assistant","model":"sandbox-model","content":[{"type":"text","text":"Parser fixed; tests remain."}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":100,"output_tokens":20}})
            }
        };
        request
            .respond(
                Response::from_string(response.to_string())
                    .with_header(content_type("application/json")),
            )
            .unwrap();
    }
}
