//! Isolated store tests: pure JSON, no sockets, no shared process state.

use super::*;
use serde_json::json;

fn call_item(call_id: &str, name: &str) -> Value {
    json!({
        "type": "function_call",
        "id": format!("fc_{call_id}"),
        "call_id": call_id,
        "name": name,
        "arguments": format!("{{\"path\":\"{name}\"}}"),
        "status": "completed"
    })
}

fn output_item(call_id: &str) -> Value {
    json!({"type":"function_call_output","call_id":call_id,"output":"done"})
}

fn rendered_response(id: &str, calls: Vec<Value>) -> Value {
    json!({"id": id, "object": "response", "status": "completed", "model": "m",
        "output": calls, "usage": {"input_tokens":1,"output_tokens":1,"total_tokens":2}, "error": null})
}

fn body_with_input(input: Value) -> Value {
    json!({"model":"m","input": input})
}

#[test]
fn short_continuation_restores_call_before_its_output() {
    let history = CodexToolHistory::default();
    let binding = HistoryBinding::default();
    history.record_response(
        &binding,
        &rendered_response("resp_1", vec![call_item("call_1", "read_file")])
            .to_string()
            .into_bytes(),
    );
    let mut body =
        json!({"model":"m","input":[output_item("call_1")],"previous_response_id":"resp_1"});
    let changed = history.enrich(&binding, &mut body).expect("enrich");
    assert_eq!(changed, 1);
    assert!(body.get("previous_response_id").is_none(), "consumed");
    assert_eq!(body["input"][0], call_item("call_1", "read_file"));
    assert_eq!(body["input"][1], output_item("call_1"));
}

#[test]
fn parallel_tool_outputs_restore_every_referenced_call() {
    let history = CodexToolHistory::default();
    let binding = HistoryBinding::default();
    history.record_response(
        &binding,
        &rendered_response(
            "resp_1",
            vec![
                call_item("call_1", "read_file"),
                call_item("call_2", "list_dir"),
            ],
        )
        .to_string()
        .into_bytes(),
    );
    let mut body = body_with_input(json!([output_item("call_2"), output_item("call_1")]));
    body["previous_response_id"] = json!("resp_1");
    assert_eq!(history.enrich(&binding, &mut body).expect("enrich"), 2);
    assert_eq!(body["input"][0], call_item("call_2", "list_dir"));
    assert_eq!(body["input"][1], output_item("call_2"));
    assert_eq!(body["input"][2], call_item("call_1", "read_file"));
    assert_eq!(body["input"][3], output_item("call_1"));
}

#[test]
fn calls_already_present_in_the_input_are_enriched_not_duplicated() {
    let history = CodexToolHistory::default();
    let binding = HistoryBinding::default();
    history.record_response(
        &binding,
        &rendered_response("resp_1", vec![call_item("call_1", "read_file")])
            .to_string()
            .into_bytes(),
    );
    let mut body = body_with_input(json!([
        {"type":"function_call","call_id":"call_1"},
        output_item("call_1"),
    ]));
    body["previous_response_id"] = json!("resp_1");
    assert_eq!(history.enrich(&binding, &mut body).expect("enrich"), 1);
    assert_eq!(body["input"].as_array().expect("array").len(), 2);
    assert_eq!(body["input"][0]["name"], "read_file");
}

#[test]
fn unknown_previous_response_id_is_a_loud_error() {
    let history = CodexToolHistory::default();
    let binding = HistoryBinding::default();
    let mut body =
        json!({"model":"m","input":[output_item("call_1")],"previous_response_id":"missing"});
    let error = history.enrich(&binding, &mut body).expect_err("unknown id");
    assert!(error.contains("previous_response_id"), "{error}");
    assert_eq!(body["input"], json!([output_item("call_1")]), "untouched");
}

#[test]
fn entries_never_cross_sessions_and_anonymous_stays_anonymous() {
    let history = CodexToolHistory::default();
    history.record_response(
        &HistoryBinding {
            session: Some("session-a".into()),
        },
        &rendered_response("resp_1", vec![call_item("call_1", "read_file")])
            .to_string()
            .into_bytes(),
    );
    let mut stranger =
        json!({"model":"m","input":[output_item("call_1")],"previous_response_id":"resp_1"});
    assert!(history
        .enrich(
            &HistoryBinding {
                session: Some("session-b".into())
            },
            &mut stranger
        )
        .is_err());
    assert!(
        stranger["input"].as_array().expect("array").len() == 1,
        "no restore"
    );
    let mut anonymous =
        json!({"model":"m","input":[output_item("call_1")],"previous_response_id":"resp_1"});
    assert!(history
        .enrich(&HistoryBinding::default(), &mut anonymous)
        .is_err());
}

#[test]
fn unique_call_fallback_serves_subagent_flows_and_conflicts_stay_ambiguous() {
    let history = CodexToolHistory::default();
    let binding = HistoryBinding::default();
    history.record_response(
        &binding,
        &rendered_response("resp_1", vec![call_item("call_1", "read_file")])
            .to_string()
            .into_bytes(),
    );
    let mut body = body_with_input(json!([output_item("call_1")]));
    assert_eq!(history.enrich(&binding, &mut body).expect("fallback"), 1);
    assert_eq!(body["input"][0], call_item("call_1", "read_file"));

    // The same call_id recorded under a second response is ambiguous.
    history.record_response(
        &binding,
        &rendered_response("resp_2", vec![call_item("call_1", "other_tool")])
            .to_string()
            .into_bytes(),
    );
    let mut ambiguous = body_with_input(json!([output_item("call_1")]));
    assert_eq!(
        history
            .enrich(&binding, &mut ambiguous)
            .expect("no restore"),
        0
    );
    assert_eq!(ambiguous["input"], json!([output_item("call_1")]));
}

#[test]
fn oldest_entries_are_evicted_once_the_cache_is_full() {
    let history = CodexToolHistory::default();
    let binding = HistoryBinding::default();
    for index in 0..MAX_ENTRIES {
        history.record_response(
            &binding,
            &rendered_response(
                &format!("resp_{index}"),
                vec![call_item(&format!("call_{index}"), "tool")],
            )
            .to_string()
            .into_bytes(),
        );
    }
    history.record_response(
        &binding,
        &rendered_response("resp_new", vec![call_item("call_new", "tool")])
            .to_string()
            .into_bytes(),
    );
    let mut evicted =
        json!({"model":"m","input":[output_item("call_0")],"previous_response_id":"resp_0"});
    assert!(
        history.enrich(&binding, &mut evicted).is_err(),
        "oldest evicted"
    );
    let mut fresh =
        json!({"model":"m","input":[output_item("call_new")],"previous_response_id":"resp_new"});
    assert_eq!(history.enrich(&binding, &mut fresh).expect("fresh"), 1);
}

#[test]
fn single_object_input_keeps_its_shape_when_nothing_changes() {
    let history = CodexToolHistory::default();
    let binding = HistoryBinding::default();
    let mut body = body_with_input(json!({"type":"message","role":"user","content":"hi"}));
    assert_eq!(history.enrich(&binding, &mut body).expect("no-op"), 0);
    assert!(body["input"].is_object(), "shape preserved");
}

#[test]
fn stream_recorder_indexes_completed_responses_only() {
    let history = Arc::new(CodexToolHistory::default());
    let binding = HistoryBinding::default();
    let mut recorder = StreamRecorder::new(history.clone(), binding.clone());
    let response = rendered_response("resp_1", vec![call_item("call_1", "read_file")]);
    recorder.feed(
        format!(
            "event: response.created\ndata: {}\n\nevent: response.completed\ndata: {}\n\n",
            json!({"type":"response.created","response":{"id":"resp_1","output":[]}}),
            json!({"type":"response.completed","response": response}),
        )
        .as_bytes(),
    );
    let partial_tail = b"data: {\"type\":\"response.completed\"";
    recorder.feed(partial_tail);
    let mut body =
        json!({"model":"m","input":[output_item("call_1")],"previous_response_id":"resp_1"});
    assert_eq!(history.enrich(&binding, &mut body).expect("recorded"), 1);
}

#[test]
fn binding_follows_body_metadata_then_headers() {
    use tiny_http::Header;
    let body = json!({"model":"m","client_metadata":{"session_id":"sess-1"},"input":[]});
    let binding = HistoryBinding::for_request(&body.to_string().into_bytes(), None);
    assert_eq!(binding.session.as_deref(), Some("sess-1"));
    let header = Header::from_bytes(b"session_id" as &[u8], b"sess-2" as &[u8]).expect("header");
    let binding = HistoryBinding::for_request(b"{}", Some(&[header]));
    assert_eq!(binding.session.as_deref(), Some("sess-2"));
    let binding = HistoryBinding::for_request(b"{}", None);
    assert_eq!(binding.session, None);
}
