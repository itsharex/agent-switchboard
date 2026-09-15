//! Isolated fixtures for the incremental Codex session usage sync.

use super::session_sync::{rebuild_with_roots, sync_with_roots};
use super::{CodexRequestLedger, CodexSessionSyncReport};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn temp() -> tempfile::TempDir {
    tempfile::tempdir().unwrap()
}

fn write_lines(path: &Path, lines: &[Value]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let body: String = lines.iter().map(|line| format!("{}\n", line)).collect();
    std::fs::write(path, body).unwrap();
}

fn meta(thread: &str, parent: Option<&str>, timestamp: &str) -> Value {
    let mut payload = json!({"id": thread});
    if let Some(parent) = parent {
        payload["forked_from_id"] = json!(parent);
    }
    json!({"timestamp": timestamp, "type": "session_meta", "payload": payload})
}

fn turn(model: &str) -> Value {
    json!({"type": "turn_context", "payload": {"model": model}})
}

/// token_count event; `total` and `last` are (input, cached, output) snapshots.
fn token(timestamp: &str, last: Option<(u64, u64, u64)>, total: Option<(u64, u64, u64)>) -> Value {
    let mut info = json!({});
    if let Some((input, cached, output)) = last {
        info["last_token_usage"] = json!({
            "input_tokens": input, "cached_input_tokens": cached, "output_tokens": output
        });
    }
    if let Some((input, cached, output)) = total {
        info["total_token_usage"] = json!({
            "input_tokens": input, "cached_input_tokens": cached, "output_tokens": output
        });
    }
    json!({
        "timestamp": timestamp, "type": "event_msg",
        "payload": {"type": "token_count", "info": info, "rate_limits": {"limit_id": "core"}}
    })
}

fn rollout_name(prefix: &str, thread: &str) -> String {
    format!("rollout-{prefix}-{thread}.jsonl")
}

fn session_summary(ledger: &CodexRequestLedger) -> (u64, u64, u64, u64) {
    let summary = ledger.summary(&Default::default()).unwrap();
    (
        summary.requests,
        summary.input_tokens,
        summary.output_tokens,
        summary.cache_read_tokens,
    )
}

#[test]
fn exact_last_usage_beats_cumulative_and_duplicate_snapshots_are_free() {
    let home = temp();
    let sessions = home.path().join("sessions");
    let thread = "11111111-1111-4111-8111-111111111111";
    let file = sessions.join(rollout_name("2026-09-14T09-00-00", thread));
    write_lines(
        &file,
        &[
            meta(thread, None, "2026-09-14T09:00:00Z"),
            turn("GLM-4.6-2026-03-05"),
            // Only a cumulative snapshot: the whole total is the first delta.
            token("2026-09-14T09:00:01Z", None, Some((100, 10, 20))),
            // Exact per-request usage wins over cumulative subtraction.
            token("2026-09-14T09:00:02Z", Some((5, 2, 3)), Some((105, 12, 23))),
            // Same total snapshot again: a rate-limit refresh, not usage.
            token("2026-09-14T09:00:03Z", None, Some((105, 12, 23))),
        ],
    );
    let root = temp();
    let ledger = CodexRequestLedger::new(root.path());
    let report = sync_with_roots(root.path(), &[sessions.clone()]);
    assert_eq!(report.errors, Vec::<String>::new());
    assert_eq!(report.imported, 2);
    assert_eq!(session_summary(&ledger), (2, 105, 23, 12));
    let records = ledger.page(&Default::default(), 0, 10).unwrap().records;
    assert!(records
        .iter()
        .all(|record| record.thread_id.as_deref() == Some(thread)
            && record.mapped_model.as_deref() == Some("glm-4.6")
            && record.status == Some(200)
            && record.profile_id.is_none()));
    // Second pass is a no-op: cursors match, nothing doubles.
    let roots = [sessions];
    let again = sync_with_roots(root.path(), &roots);
    assert_eq!(again.imported, 0);
    assert_eq!(session_summary(&ledger), (2, 105, 23, 12));
}

#[test]
fn appended_events_sync_when_mtime_is_unchanged() {
    let home = temp();
    let sessions = home.path().join("sessions");
    let thread = "22222222-2222-4222-8222-222222222222";
    let file = sessions.join(rollout_name("2026-09-14T10-00-00", thread));
    write_lines(
        &file,
        &[
            meta(thread, None, "2026-09-14T10:00:00Z"),
            token("2026-09-14T10:00:01Z", Some((10, 0, 5)), None),
        ],
    );
    let root = temp();
    let ledger = CodexRequestLedger::new(root.path());
    sync_with_roots(root.path(), &[sessions.clone()]);
    assert_eq!(session_summary(&ledger).0, 1);

    // Append another event, then force the persisted mtime to match the new
    // file exactly: Windows can leave mtime unchanged while Codex writes.
    write_lines(
        &file,
        &[
            meta(thread, None, "2026-09-14T10:00:00Z"),
            token("2026-09-14T10:00:01Z", Some((10, 0, 5)), None),
            token("2026-09-14T10:05:00Z", Some((7, 0, 2)), None),
        ],
    );
    let modified: i64 = std::fs::metadata(&file)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64;
    let db = rusqlite::Connection::open(&ledger.path).unwrap();
    db.execute(
        "UPDATE codex_session_sync SET modified_nanos=?1",
        [modified],
    )
    .unwrap();
    drop(db);
    let report = sync_with_roots(root.path(), &[sessions]);
    assert_eq!(report.imported, 1);
    assert_eq!(session_summary(&ledger).0, 2);
}

#[test]
fn truncation_never_double_counts_and_rows_survive() {
    let home = temp();
    let sessions = home.path().join("sessions");
    let thread = "33333333-3333-4333-8333-333333333333";
    let file = sessions.join(rollout_name("2026-09-14T11-00-00", thread));
    write_lines(
        &file,
        &[
            meta(thread, None, "2026-09-14T11:00:00Z"),
            token("2026-09-14T11:00:01Z", Some((10, 0, 5)), None),
            token("2026-09-14T11:00:02Z", Some((6, 0, 1)), None),
        ],
    );
    let root = temp();
    let ledger = CodexRequestLedger::new(root.path());
    sync_with_roots(root.path(), &[sessions.clone()]);
    assert_eq!(session_summary(&ledger).0, 2);

    // The file is rewritten shorter; billed history stays, nothing doubles.
    write_lines(
        &file,
        &[
            meta(thread, None, "2026-09-14T11:00:00Z"),
            token("2026-09-14T11:00:01Z", Some((10, 0, 5)), None),
        ],
    );
    let report = sync_with_roots(root.path(), &[sessions]);
    assert_eq!(report.imported, 0);
    assert_eq!(session_summary(&ledger).0, 2);
}

#[test]
fn forked_rollout_only_bills_events_after_the_replayed_prefix() {
    let home = temp();
    let sessions = home.path().join("sessions");
    let parent = "44444444-4444-4444-8444-444444444444";
    let child = "55555555-5555-4555-8555-555555555555";
    let parent_file = sessions.join(rollout_name("2026-09-14T12-00-00", parent));
    write_lines(
        &parent_file,
        &[
            meta(parent, None, "2026-09-14T12:00:00Z"),
            token("2026-09-14T12:00:01Z", Some((10, 0, 4)), None),
            token("2026-09-14T12:00:02Z", Some((8, 0, 2)), None),
        ],
    );
    let child_file = sessions.join(rollout_name("2026-09-14T12-05-00", child));
    write_lines(
        &child_file,
        &[
            meta(child, Some(parent), "2026-09-14T12:00:02Z"),
            // Replayed parent turns: identical snapshots, before the fork.
            token("2026-09-14T12:05:01Z", Some((10, 0, 4)), None),
            token("2026-09-14T12:05:02Z", Some((8, 0, 2)), None),
            // The fork's own new turn.
            token("2026-09-14T12:06:00Z", Some((3, 0, 1)), None),
        ],
    );
    let root = temp();
    let ledger = CodexRequestLedger::new(root.path());
    let report = sync_with_roots(root.path(), &[sessions]);
    assert_eq!(report.imported, 3, "parent 2 + child 1");
    assert_eq!(session_summary(&ledger).0, 3);
    let records = ledger.page(&Default::default(), 0, 10).unwrap().records;
    let child_rows: Vec<_> = records
        .iter()
        .filter(|record| record.thread_id.as_deref() == Some(child))
        .collect();
    assert_eq!(child_rows.len(), 1);
    assert_eq!(child_rows[0].input_tokens, Some(3));
}

#[test]
fn missing_parent_defers_until_it_appears() {
    let home = temp();
    let sessions = home.path().join("sessions");
    let parent = "66666666-6666-4666-8666-666666666666";
    let child = "77777777-7777-4777-8777-777777777777";
    let child_file = sessions.join(rollout_name("2026-09-14T13-00-00", child));
    write_lines(
        &child_file,
        &[
            meta(child, Some(parent), "2026-09-14T12:50:01Z"),
            token("2026-09-14T12:50:30Z", Some((4, 0, 1)), None),
        ],
    );
    let root = temp();
    let ledger = CodexRequestLedger::new(root.path());
    let roots = [sessions.clone()];
    let report = sync_with_roots(root.path(), &roots);
    assert_eq!(report.deferred_files, 1);
    assert_eq!(report.imported, 0);
    assert_eq!(session_summary(&ledger).0, 0);

    let parent_file = sessions.join(rollout_name("2026-09-14T12-50-00", parent));
    write_lines(
        &parent_file,
        &[
            meta(parent, None, "2026-09-14T12:50:00Z"),
            token("2026-09-14T12:50:01Z", Some((9, 0, 2)), None),
        ],
    );
    let report = sync_with_roots(root.path(), &roots);
    assert_eq!(report.deferred_files, 0);
    assert_eq!(report.imported, 2);
    assert_eq!(session_summary(&ledger).0, 2);
}

#[test]
fn revert_replacement_rollouts_keep_thread_identity_under_the_new_id() {
    let home = temp();
    let sessions = home.path().join("sessions");
    let thread = "88888888-8888-4888-8888-888888888888";
    let rollout = "99999999-9999-4999-8999-999999999999";
    // Replacement rollout file name: …<thread>_<rollout>.jsonl; root meta
    // carries the original thread id.
    let file = sessions.join(format!(
        "rollout-2026-09-14T14-00-00-{thread}_{rollout}.jsonl"
    ));
    write_lines(
        &file,
        &[
            meta(thread, None, "2026-09-14T14:00:00Z"),
            token("2026-09-14T14:00:01Z", Some((12, 0, 6)), None),
        ],
    );
    let root = temp();
    let ledger = CodexRequestLedger::new(root.path());
    let report = sync_with_roots(root.path(), &[sessions]);
    assert_eq!(report.imported, 1);
    let record = &ledger.page(&Default::default(), 0, 10).unwrap().records[0];
    assert_eq!(record.thread_id.as_deref(), Some(thread));
    assert_eq!(record.id, format!("codex-session-v1:{rollout}:1"));
}

#[test]
fn archived_moves_inherit_the_sessions_cursor() {
    let home = temp();
    let sessions = home.path().join("sessions");
    let archived = home.path().join("archived_sessions");
    let thread = "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa";
    let name = rollout_name("2026-09-14T15-00-00", thread);
    let live = sessions.join(&name);
    write_lines(
        &live,
        &[
            meta(thread, None, "2026-09-14T15:00:00Z"),
            token("2026-09-14T15:00:01Z", Some((5, 0, 2)), None),
        ],
    );
    let root = temp();
    let ledger = CodexRequestLedger::new(root.path());
    let roots_now: Vec<PathBuf> = vec![sessions.clone(), archived.clone()];
    sync_with_roots(root.path(), &roots_now);
    assert_eq!(session_summary(&ledger).0, 1);

    // Codex archives the file: same bytes, new directory, unchanged mtime.
    std::fs::create_dir_all(&archived).unwrap();
    std::fs::rename(&live, archived.join(&name)).unwrap();
    let report = sync_with_roots(root.path(), &roots_now);
    assert_eq!(report.imported, 0);
    assert_eq!(session_summary(&ledger).0, 1);
}

#[test]
fn turns_recorded_by_the_gateway_are_not_billed_twice() {
    let home = temp();
    let sessions = home.path().join("sessions");
    let thread = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
    let file = sessions.join(rollout_name("2026-09-14T16-00-00", thread));
    write_lines(
        &file,
        &[
            meta(thread, None, "2026-09-14T16:00:00Z"),
            token("2026-09-14T16:00:01Z", Some((10, 2, 4)), None),
        ],
    );
    let root = temp();
    let ledger = CodexRequestLedger::new(root.path());
    // The same turn was already recorded by the proxy at the same instant.
    let proxied = super::tests::proxy_record(
        "2026-09-14T16:00:01Z",
        "gpt-5.4",
        Some(10),
        Some(4),
        Some(2),
    );
    ledger.append(&proxied).unwrap();
    let roots = [sessions];
    let report = sync_with_roots(root.path(), &roots);
    assert_eq!(report.imported, 0);
    assert_eq!(report.skipped, 1);
    assert_eq!(session_summary(&ledger).0, 1);

    // A different session turn still imports normally.
    write_lines(
        &file,
        &[
            meta(thread, None, "2026-09-14T16:00:00Z"),
            token("2026-09-14T16:00:01Z", Some((10, 2, 4)), None),
            token("2026-09-14T16:10:00Z", Some((9, 1, 3)), None),
        ],
    );
    let report = sync_with_roots(root.path(), &roots);
    assert_eq!(report.imported, 1);
    assert_eq!(session_summary(&ledger).0, 2);
}

#[test]
fn rebuild_backs_up_then_reimports_only_session_rows() {
    let home = temp();
    let sessions = home.path().join("sessions");
    let thread = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
    let file = sessions.join(rollout_name("2026-09-14T17-00-00", thread));
    write_lines(
        &file,
        &[
            meta(thread, None, "2026-09-14T17:00:00Z"),
            token("2026-09-14T17:00:01Z", Some((11, 0, 3)), None),
        ],
    );
    let root = temp();
    let ledger = CodexRequestLedger::new(root.path());
    let proxied = super::tests::proxy_record(
        "2026-09-14T17:30:00Z",
        "gpt-5.4",
        Some(50),
        Some(5),
        Some(0),
    );
    ledger.append(&proxied).unwrap();
    sync_with_roots(root.path(), &[sessions.clone()]);
    assert_eq!(session_summary(&ledger).0, 2);

    let (backup, report) = rebuild_with_roots(root.path(), &[sessions.clone()]).unwrap();
    let backup = backup.expect("ledger existed, backup taken");
    assert!(backup.is_file());
    assert!(backup
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.starts_with("requests-backup-")));
    assert_eq!(report.imported, 1);
    // The proxy row survived the rebuild; the session row was reimported.
    assert_eq!(session_summary(&ledger).0, 2);
    let records = ledger.page(&Default::default(), 0, 10).unwrap().records;
    assert!(records.iter().any(|record| record.id == proxied.id));
    assert_eq!(
        records
            .iter()
            .filter(|record| record.origin == super::CodexUsageOrigin::Session)
            .count(),
        1
    );
}

#[test]
fn session_records_reject_gateway_fields_and_unknown_ids() {
    let thread = "dddddddd-dddd-4ddd-8ddd-dddddddddddd";
    let mut record = super::tests::session_record_shape(thread, 1);
    record.validate().unwrap();
    record.profile_id = Some("provider".into());
    assert!(record.validate().is_err());
    record.profile_id = None;
    record.id = format!("codex-session-v1:{thread}:not-a-number");
    assert!(record.validate().is_err());
    record.id = format!("codex-session-v1:eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee:2");
    assert!(
        record.validate().is_ok(),
        "id and thread are distinct identities"
    );
}

#[test]
fn v1_ledgers_migrate_in_place_with_origin_markers() {
    let root = temp();
    let ledger = CodexRequestLedger::new(root.path());
    let db_path = ledger.path.clone();
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    {
        use rusqlite::Connection;
        let db = Connection::open(&db_path).unwrap();
        db.execute_batch(
            "CREATE TABLE codex_requests (
              id TEXT PRIMARY KEY NOT NULL, at_ms INTEGER NOT NULL CHECK(at_ms >= 0),
              profile_id TEXT, model TEXT, status INTEGER CHECK(status BETWEEN 100 AND 599),
              input_tokens INTEGER, output_tokens INTEGER, cache_read_tokens INTEGER,
              cache_creation_tokens INTEGER, reasoning_tokens INTEGER, cost_micros INTEGER,
              payload TEXT NOT NULL CHECK(json_valid(payload))
             );
             PRAGMA user_version = 1;",
        )
        .unwrap();
        let payload =
            super::tests::proxy_record("2026-09-14T18:00:00Z", "gpt-5.4", None, None, None);
        let text = {
            let mut value = serde_json::to_value(&payload).unwrap();
            value.as_object_mut().unwrap().remove("origin");
            value.to_string()
        };
        db.execute(
            "INSERT INTO codex_requests (id,at_ms,profile_id,model,status,input_tokens,
             output_tokens,cache_read_tokens,cache_creation_tokens,reasoning_tokens,cost_micros,payload)
             VALUES (?1,?2,NULL,?3,200,NULL,NULL,NULL,NULL,NULL,NULL,?4)",
            rusqlite::params![payload.id, payload.at_ms, payload.mapped_model, text],
        )
        .unwrap();
    }
    let page = ledger.page(&Default::default(), 0, 10).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.records[0].origin, super::CodexUsageOrigin::Proxy);
    let db = rusqlite::Connection::open(&db_path).unwrap();
    let version: i64 = db
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 2);
    let sessions: i64 = db
        .query_row(
            "SELECT count(*) FROM codex_requests WHERE origin='session'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(sessions, 0);
}

#[test]
fn report_shape_is_stable_for_the_frontend() {
    let report = CodexSessionSyncReport::default();
    let encoded = serde_json::to_value(&report).unwrap();
    assert_eq!(
        serde_json::from_value::<Vec<String>>(encoded["errors"].clone()).unwrap(),
        Vec::<String>::new()
    );
    for key in [
        "filesScanned",
        "imported",
        "skipped",
        "suspectedDuplicates",
        "deferredFiles",
        "errors",
    ] {
        assert!(encoded.get(key).is_some(), "missing {key}");
    }
}
