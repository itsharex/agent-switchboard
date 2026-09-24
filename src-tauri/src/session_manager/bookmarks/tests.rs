use super::*;
use std::fs;
use tempfile::tempdir;

fn snapshot() -> (SessionMeta, SessionMessage) {
    (SessionMeta { app: AppKind::Codex, session_id: "s1".into(), title: "会话标题".into(),
        summary: "摘要".into(), project_dir: Some("/project".into()),
        created_at: None, last_active_at: None, resume_command: String::new(),
        alias: None, pinned: false, tags: Vec::new() },
    SessionMessage { id: "message-hash".into(), role: "assistant".into(),
        content: "完整消息\n```rust\nlet x = 1;\n```".into(), at: None })
}

#[test]
fn bookmarks_are_idempotent_persistent_snapshots() {
    let temp = tempdir().unwrap();
    assert!(list_bookmarks(temp.path()).unwrap().is_empty());
    assert!(!database_path(temp.path()).exists());
    let (meta, message) = snapshot();
    let first = save_snapshot(temp.path(), &meta, &message).unwrap();
    let mut changed = meta.clone();
    changed.title = "新标题".into();
    let second = save_snapshot(temp.path(), &changed, &message).unwrap();
    assert_eq!(first.id, second.id);
    assert_eq!(first.saved_at, second.saved_at);
    assert_eq!(second.session_title, meta.title);
    let rows = list_bookmarks(temp.path()).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].content, message.content);
    delete_bookmark(temp.path(), &first.id).unwrap();
    assert!(list_bookmarks(temp.path()).unwrap().is_empty());
}

#[test]
fn rejects_system_messages_and_does_not_replace_invalid_database() {
    let temp = tempdir().unwrap();
    let (meta, mut message) = snapshot();
    message.role = "system".into();
    assert!(save_snapshot(temp.path(), &meta, &message).is_err());
    let path = database_path(temp.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"corrupted database").unwrap();
    assert!(list_bookmarks(temp.path()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"corrupted database");
}

#[test]
fn rejects_foreign_schema_without_migration() {
    let temp = tempdir().unwrap();
    let path = database_path(temp.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let connection = Connection::open(&path).unwrap();
    connection.execute_batch("CREATE TABLE previous(content TEXT); PRAGMA user_version=9;").unwrap();
    drop(connection);
    assert!(list_bookmarks(temp.path()).unwrap_err().contains("结构无效"));
    let connection = Connection::open(&path).unwrap();
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0)).unwrap();
    assert_eq!(version, 9);
}

#[test]
fn concurrent_duplicate_saves_keep_one_row() {
    let temp = tempdir().unwrap();
    let (meta, message) = snapshot();
    let root = temp.path().to_path_buf();
    let barrier = std::sync::Barrier::new(2);
    std::thread::scope(|scope| {
        let first = scope.spawn(|| {
            barrier.wait();
            save_snapshot(&root, &meta, &message).unwrap()
        });
        let second = scope.spawn(|| {
            barrier.wait();
            save_snapshot(&root, &meta, &message).unwrap()
        });
        assert_eq!(first.join().unwrap().id, second.join().unwrap().id);
    });
    assert_eq!(list_bookmarks(temp.path()).unwrap().len(), 1);
}

#[test]
fn rejects_existing_empty_database_without_initializing_it() {
    let temp = tempdir().unwrap();
    let path = database_path(temp.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, b"").unwrap();
    let (meta, message) = snapshot();
    assert!(save_snapshot(temp.path(), &meta, &message).unwrap_err().contains("结构无效"));
    assert_eq!(fs::metadata(path).unwrap().len(), 0);
}

#[test]
fn stored_schema_matches_the_single_current_definition_after_reopening() {
    let temp = tempdir().unwrap();
    let connection = open_store(temp.path()).unwrap();
    let stored: String = connection.query_row(
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='bookmarks'",
        [], |row| row.get(0),
    ).unwrap();
    assert_eq!(stored, SCHEMA);
    drop(connection);
    open_store(temp.path()).unwrap();
}

#[test]
fn parsed_tool_output_cannot_be_bookmarked() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("session.jsonl");
    fs::write(&path, concat!(
        "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"output\":\"error text\"}}\n",
        "{\"type\":\"response_item\",\"payload\":{\"type\":\"custom_tool_call_output\",\"output\":\"patch error\"}}\n",
    )).unwrap();
    let (meta, _) = snapshot();
    for message in read_messages(&path).unwrap() {
        assert_eq!(message.role, "tool");
        assert!(save_snapshot(temp.path(), &meta, &message).unwrap_err().contains("只能收藏"));
    }
    assert!(!database_path(temp.path()).exists());
}
