use super::*;
use crate::gateway::request_ledger::ClaudeRequestLedger;
use serde_json::json;

fn write_lines(path: &Path, lines: &[Value]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let body: String = lines.iter().map(|line| format!("{line}\n")).collect();
    std::fs::write(path, body).unwrap();
}

fn assistant(
    id: &str,
    at: &str,
    model: &str,
    usage: (u64, u64, u64, u64),
    stop: Option<&str>,
) -> Value {
    let (input, output, read, create) = usage;
    let mut message = json!({
        "id": id, "model": model,
        "usage": {"input_tokens": input, "output_tokens": output,
                  "cache_read_input_tokens": read, "cache_creation_input_tokens": create}
    });
    if let Some(stop) = stop {
        message["stop_reason"] = json!(stop);
    }
    json!({"type": "assistant", "timestamp": at, "sessionId": "sess-1", "message": message})
}

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("state");
    std::fs::create_dir_all(&root).unwrap();
    let projects = dir.path().join("projects");
    (dir, root, projects)
}

#[test]
fn assistant_messages_are_imported_once_priced_and_superseded_by_the_final_line() {
    let (_dir, root, projects) = fixture();
    let file = projects.join("repo").join("a.jsonl");
    write_lines(
        &file,
        &[
            json!({"type": "user", "message": {"role": "user"}}),
            assistant(
                "msg-1",
                "2026-09-14T01:00:00Z",
                "claude-sonnet-5",
                (100, 5, 20, 10),
                None,
            ),
            assistant(
                "msg-1",
                "2026-09-14T01:00:02Z",
                "claude-sonnet-5",
                (100, 40, 20, 10),
                Some("end_turn"),
            ),
            assistant(
                "msg-2",
                "2026-09-14T01:01:00Z",
                "custom-model",
                (10, 1, 0, 0),
                Some("end_turn"),
            ),
            json!({"type": "assistant", "message": {"id": "msg-3", "usage": {"input_tokens": 0, "output_tokens": 0}}, "timestamp": "2026-09-14T01:02:00Z"}),
            json!("not an object"),
        ],
    );
    let report = sync_with_roots(&root, &[projects.clone()]);
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(
        (report.files_scanned, report.imported, report.updated),
        (1, 2, 1)
    );

    let summary = summary(&root).unwrap();
    assert_eq!(summary.requests, 2);
    // Input includes both cache classes so it matches the gateway ledger shape.
    assert_eq!(summary.input_tokens, 130 + 10);
    assert_eq!(summary.output_tokens, 41);
    assert_eq!(summary.cache_read_tokens, 20);
    assert_eq!(summary.priced_requests, 1);
    let sonnet = summary
        .by_model
        .iter()
        .find(|bucket| bucket.model == "claude-sonnet-5")
        .unwrap();
    // 100 fresh × $2 + 40 out × $10 + 20 read × $0.2 + 10 create × $2.5 per million
    // (the built-in v2 reference seed owns sonnet-5 pricing).
    assert_eq!(sonnet.estimated_cost_usd, "0.000629");
    let custom = summary
        .by_model
        .iter()
        .find(|bucket| bucket.model == "custom-model")
        .unwrap();
    assert_eq!(custom.priced_requests, 0);
    assert_eq!(custom.estimated_cost_usd, "0.000000");

    // A second pass with an unchanged file imports nothing.
    let again = sync_with_roots(&root, &[projects]);
    assert_eq!((again.imported, again.updated), (0, 0));
}

#[test]
fn appends_are_incremental_and_rewrites_pin_without_replaying() {
    let (_dir, root, projects) = fixture();
    let file = projects.join("repo").join("a.jsonl");
    write_lines(
        &file,
        &[assistant(
            "msg-1",
            "2026-09-14T01:00:00Z",
            "m",
            (10, 1, 0, 0),
            Some("end_turn"),
        )],
    );
    assert_eq!(sync_with_roots(&root, &[projects.clone()]).imported, 1);

    let mut appended = std::fs::read_to_string(&file).unwrap();
    appended.push_str(&format!(
        "{}\n",
        assistant(
            "msg-2",
            "2026-09-14T01:00:05Z",
            "m",
            (10, 1, 0, 0),
            Some("end_turn")
        )
    ));
    // A trailing partial line is left for the next pass.
    appended.push_str("{\"type\":\"assistant\"");
    std::fs::write(&file, appended).unwrap();
    let report = sync_with_roots(&root, &[projects.clone()]);
    assert_eq!(report.imported, 1);
    assert_eq!(summary(&root).unwrap().requests, 2);

    // Rewriting earlier content pins the cursor at the new end: nothing is
    // replayed and nothing already counted is lost.
    write_lines(
        &file,
        &[assistant(
            "msg-9",
            "2026-09-14T02:00:00Z",
            "m",
            (10, 1, 0, 0),
            Some("end_turn"),
        )],
    );
    let report = sync_with_roots(&root, &[projects.clone()]);
    assert_eq!(report.pinned_rewrites, 1);
    assert_eq!(report.imported, 0);
    assert_eq!(summary(&root).unwrap().requests, 2);

    let (backup, rebuilt) = rebuild_with_roots(&root, &[projects]).unwrap();
    assert!(backup.is_some_and(|name| root.join(BACKUP_DIR).join(name).is_file()));
    assert_eq!(rebuilt.imported, 1);
    assert_eq!(summary(&root).unwrap().requests, 1);
}

#[test]
fn a_gateway_request_with_the_same_usage_inside_the_window_is_marked_matched() {
    let (_dir, root, projects) = fixture();
    let ledger = ClaudeRequestLedger::new(root.join("claude-request-ledger.json"));
    ledger
        .append(ClaudeRequestRecord {
            at: "2026-09-14T01:03:00Z".into(),
            profile_id: Some("relay".into()),
            route_revision: Some("r1".into()),
            client_protocol: UpstreamProtocol::AnthropicMessages,
            upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
            request_model: Some("claude-sonnet-5".into()),
            mapped_model: Some("vendor-sonnet".into()),
            response_model: Some("claude-sonnet-5".into()),
            cost: None,
            pricing_error: None,
            first_token_latency_ms: None,
            input_tokens: Some(130),
            output_tokens: Some(40),
            cache_read_tokens: Some(20),
            cache_creation_tokens: Some(10),
            reasoning_tokens: None,
            status: Some(200),
            duration_ms: 5,
            first_byte_latency_ms: None,
            failover_attempts: Vec::new(),
        })
        .unwrap();
    let file = projects.join("repo").join("a.jsonl");
    write_lines(
        &file,
        &[
            assistant(
                "in-window",
                "2026-09-14T01:00:00Z",
                "claude-sonnet-5",
                (100, 40, 20, 10),
                Some("end_turn"),
            ),
            assistant(
                "out-of-window",
                "2026-09-14T00:30:00Z",
                "claude-sonnet-5",
                (100, 40, 20, 10),
                Some("end_turn"),
            ),
            assistant(
                "other-usage",
                "2026-09-14T01:03:00Z",
                "claude-sonnet-5",
                (100, 41, 20, 10),
                Some("end_turn"),
            ),
        ],
    );
    let report = sync_with_roots(&root, &[projects]);
    assert_eq!(report.imported, 3);
    assert_eq!(report.gateway_matched, 1);
    let summary = summary(&root).unwrap();
    assert_eq!(summary.gateway_matched, 1);
    let stored = load(&root).unwrap();
    let matched: Vec<_> = stored
        .entries
        .iter()
        .filter(|entry| entry.gateway_matched)
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(matched, ["session:in-window"]);
}
