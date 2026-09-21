use super::*;
use crate::codex_probe::tests::write_rollout;

const ID: &str = "11111111-1111-4111-8111-111111111111";
const OTHER: &str = "22222222-2222-4222-8222-222222222222";

#[test]
fn events_require_one_valid_thread_identity_and_a_completed_turn() {
    let output = format!(r#"{{"type":"thread.started","thread_id":"{ID}"}}
{{"type":"item.completed","item":{{"type":"agent_message","text":"45"}}}}
{{"type":"turn.completed"}}"#);
    let events = parse_events(&output).unwrap();
    assert_eq!(events.session_id, ID);
    assert!(events.completed);
    assert!(events.failure.is_none());
    assert!(parse_events("45").is_err());
    assert!(parse_events(r#"{"type":"turn.completed"}"#).is_err());
    assert!(parse_events(r#"{"type":"thread.started","thread_id":"../../other"}"#).is_err());
    assert!(parse_events(&format!("{output}\n{}", serde_json::json!({
        "type": "thread.started", "thread_id": OTHER,
    }))).is_err());
    let failed = parse_events(&format!("{output}\n{}", serde_json::json!({
        "type": "turn.failed", "error": {"message": "request failed"},
    }))).unwrap();
    assert_eq!(failed.failure.as_deref(), Some("request failed"));
}

#[test]
fn usage_comes_only_from_the_exact_session_even_with_concurrent_sessions() {
    let root = tempfile::tempdir().unwrap();
    write_rollout(root.path(), ID, 190);
    write_rollout(root.path(), OTHER, 99999);
    let usage = read_session_usage(root.path(), ID).unwrap();
    assert_eq!(usage.total, Some(190));
    assert_eq!(usage.reasoning_output, Some(77));
    assert!(read_session_usage(root.path(), "33333333-3333-4333-8333-333333333333").is_err());
}

#[test]
fn mismatched_metadata_and_ambiguous_files_never_supply_usage() {
    let root = tempfile::tempdir().unwrap();
    let path = write_rollout(root.path(), ID, 190);
    let source = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, source.replace(ID, OTHER)).unwrap();
    assert!(read_session_usage(root.path(), ID).is_err());
    std::fs::write(&path, &source).unwrap();
    let archived = root.path().join("archived_sessions");
    std::fs::create_dir(&archived).unwrap();
    std::fs::write(archived.join(path.file_name().unwrap()), source).unwrap();
    assert!(read_session_usage(root.path(), ID).is_err());
}
