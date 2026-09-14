use super::*;

fn compaction_cases() -> [(Option<&'static str>, Option<u64>, u64); 8] {
    [
        (None, None, 8_192),
        (Some(DEFAULT_MODEL), None, 8_192),
        (Some(SMALL_MODEL), None, 4_096),
        (Some(TINY_MODEL), None, 1_024),
        (Some(SMALL_MODEL), Some(768), 768),
        (Some(TINY_MODEL), Some(512), 512),
        (Some(DEFAULT_MODEL), Some(12_000), 8_192),
        (Some(DEFAULT_MODEL), Some(1_536), 1_536),
    ]
}

fn compact_body(model: Option<&str>, budget: Option<u64>, v2: bool) -> Value {
    let mut body = budget_request(model, budget.map(Value::from));
    if v2 {
        body["input"]
            .as_array_mut()
            .unwrap()
            .push(json!({"type": "compaction_trigger"}));
        body["stream"] = json!(true);
    }
    body
}

#[test]
fn http_legacy_and_v2_compaction_use_each_client_models_budget() {
    for protocol in [
        CodexUpstream::ChatCompletions,
        CodexUpstream::AnthropicMessages,
    ] {
        let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
        let sandbox = BudgetSandbox::new(&upstream, protocol);
        let cases = compaction_cases();
        let (observed, worker) =
            serve_budget_upstream(upstream, protocol.protocol(), cases.len() * 2);
        for v2 in [false, true] {
            for (model, explicit, expected) in cases {
                let operation = if v2 {
                    "/responses"
                } else {
                    "/responses/compact"
                };
                let response = sandbox.post(operation, &compact_body(model, explicit, v2));
                let status = response.status().as_u16();
                let text = response.text().expect("compaction response");
                assert_eq!(status, 200, "model {model:?}, v2 {v2}: {text}");
                if v2 {
                    assert!(text.contains("event: response.completed"));
                } else {
                    let body: Value = serde_json::from_str(&text).expect("compact JSON");
                    assert_eq!(body["output"][0]["type"], "compaction");
                }
                let body = observed_body(&observed);
                assert_converted_budget(&body, model, expected);
                assert_eq!(body["stream"], false, "summary requests are non-streaming");
            }
        }
        worker.join().expect("upstream worker");
    }
}

#[test]
fn websocket_v2_compaction_uses_each_client_models_budget_for_both_bridges() {
    for protocol in [
        CodexUpstream::ChatCompletions,
        CodexUpstream::AnthropicMessages,
    ] {
        let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
        let sandbox = BudgetSandbox::new(&upstream, protocol);
        let cases = compaction_cases();
        let (observed, worker) = serve_budget_upstream(upstream, protocol.protocol(), cases.len());
        let mut socket = sandbox.socket();
        for (model, explicit, expected) in cases {
            let model = model.unwrap_or(DEFAULT_MODEL);
            let mut body = compact_body(Some(model), explicit, true);
            body["type"] = json!("response.create");
            body["store"] = json!(false);
            send_websocket_text(&mut socket, &body.to_string());
            let completed = receive_responses_completion(&mut socket);
            assert_eq!(completed["response"]["output"][0]["type"], "compaction");
            let body = observed_body(&observed);
            assert_converted_budget(&body, Some(model), expected);
            assert_eq!(body["stream"], false, "summary requests are non-streaming");
        }
        drop(socket);
        worker.join().expect("upstream worker");
    }
}

fn admit(
    sandbox: &BudgetSandbox,
    operation: CodexOperation,
    body: &Value,
) -> Result<Value, String> {
    let crate::gateway::GatewayActivation::Routed(route) = &sandbox.projection.activation else {
        panic!("budget sandbox requires a routed gateway");
    };
    super::super::super::codex::resolve_model_and_validate(
        route,
        operation,
        body.to_string().into_bytes(),
    )
    .map(|bytes| serde_json::from_slice(&bytes).expect("admitted JSON"))
}

#[test]
fn compaction_admission_rejects_invalid_and_over_catalog_explicit_budgets() {
    for protocol in [
        CodexUpstream::ChatCompletions,
        CodexUpstream::AnthropicMessages,
    ] {
        let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
        let sandbox = BudgetSandbox::new(&upstream, protocol);
        for (operation, v2) in [
            (CodexOperation::Compact, false),
            (CodexOperation::Responses, true),
        ] {
            for budget in invalid_budgets() {
                let mut body = compact_body(Some(SMALL_MODEL), None, v2);
                body["max_output_tokens"] = budget;
                let error = admit(&sandbox, operation, &body).expect_err("invalid budget rejected");
                assert!(error.contains("max_output_tokens"));
            }
            let body = compact_body(Some(TINY_MODEL), Some(2_048), v2);
            let error = admit(&sandbox, operation, &body).expect_err("tiny model budget rejected");
            assert!(error.contains(TINY_MODEL));
            assert!(error.contains("1024"));
        }
    }
}

#[test]
fn native_responses_compaction_admission_preserves_original_fields() {
    let upstream = Server::http(("127.0.0.1", 0)).expect("upstream listener");
    let sandbox = BudgetSandbox::new(&upstream, CodexUpstream::Responses);
    for (operation, v2) in [
        (CodexOperation::Compact, false),
        (CodexOperation::Responses, true),
    ] {
        for budget in [None, Some(json!(32_768)), Some(Value::Null)] {
            let mut body = compact_body(Some(TINY_MODEL), None, v2);
            if let Some(budget) = budget {
                body["max_output_tokens"] = budget;
            }
            body["metadata"] = json!({"native_marker": "preserved"});
            let admitted = admit(&sandbox, operation, &body).expect("native request admitted");
            body["model"] = json!(DEFAULT_MODEL);
            assert_eq!(
                admitted, body,
                "native compaction must not acquire bridge fields"
            );
        }
    }
}
