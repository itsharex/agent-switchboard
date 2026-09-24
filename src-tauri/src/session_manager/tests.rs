use super::*;
use tempfile::tempdir;

struct ScanEvidence {
    sessions: Vec<SessionMeta>,
    issues: Vec<SessionIssue>,
}

fn scan_session_roots(roots: &[(AppKind, PathBuf)], titles: &HashMap<String, String>) -> ScanEvidence {
    let (sources, issues) = scan_session_source_roots(roots, titles);
    ScanEvidence { sessions: sources.into_iter().map(|source| source.meta).collect(), issues }
}

fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(path, content).expect("write session");
}

#[test]
fn scans_supported_sources_without_exposing_paths() {
    let temp = tempdir().expect("temp");
    let codex = temp.path().join("codex").join("a.jsonl");
    let claude = temp.path().join("claude").join("project").join("b.jsonl");
    write(
        &codex,
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-123\",\"cwd\":\"C:/work/relay\",\"timestamp\":\"2026-08-01T10:00:00Z\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":\"修复会话页面\"}}\n"
        ),
    );
    write(
        &claude,
        concat!(
            "{\"type\":\"user\",\"sessionId\":\"claude-456\",\"cwd\":\"C:/work/claude\",\"timestamp\":\"2026-08-02T10:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"整理历史\"}}\n",
            "{\"type\":\"custom-title\",\"customTitle\":\"归档整理\"}\n"
        ),
    );

    let scan = scan_session_roots(
        &[
            (AppKind::Codex, temp.path().join("codex")),
            (AppKind::Claude, temp.path().join("claude")),
        ],
        &HashMap::new(),
    );

    assert!(scan.issues.is_empty());
    assert_eq!(scan.sessions.len(), 2);
    let claude = scan
        .sessions
        .iter()
        .find(|session| session.app == AppKind::Claude)
        .expect("Claude session");
    let codex = scan
        .sessions
        .iter()
        .find(|session| session.app == AppKind::Codex)
        .expect("Codex session");
    assert_eq!(claude.title, "归档整理");
    assert_eq!(claude.resume_command, "claude --resume claude-456");
    assert_eq!(codex.resume_command, "codex resume codex-123");
}

#[test]
fn claude_discovery_excludes_nested_workflow_journals() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("projects");
    let project = root.join("encoded-project");
    write(
        &project.join("claude-session.jsonl"),
        "{\"type\":\"user\",\"sessionId\":\"claude-session\",\"message\":{\"role\":\"user\",\"content\":\"主会话\"}}\n",
    );
    write(
        &project.join("claude-session").join("subagents").join("workflows")
            .join("wf-example").join("journal.jsonl"),
        "{\"type\":\"workflow-journal\",\"content\":\"internal\"}\n",
    );

    let scan = scan_session_roots(&[(AppKind::Claude, root)], &HashMap::new());

    assert!(scan.issues.is_empty());
    assert_eq!(scan.sessions.len(), 1);
    assert_eq!(scan.sessions[0].session_id, "claude-session");
}

#[test]
fn duplicate_session_ids_keep_the_first_configured_source() {
    let temp = tempdir().expect("temp");
    let active_root = temp.path().join("codex").join("sessions");
    let archived_root = temp.path().join("codex").join("archived_sessions");
    write(
        &active_root.join("active.jsonl"),
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-duplicate\",\"cwd\":\"C:/work/relay\",\"timestamp\":\"2026-08-01T10:00:00Z\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":\"活动会话\"}}\n"
        ),
    );
    write(
        &archived_root.join("archived.jsonl"),
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-duplicate\",\"cwd\":\"C:/work/relay\",\"timestamp\":\"2026-07-01T10:00:00Z\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":\"归档副本\"}}\n"
        ),
    );

    let scan = scan_session_roots(
        &[
            (AppKind::Codex, active_root.clone()),
            (AppKind::Codex, archived_root),
        ],
        &HashMap::new(),
    );

    assert_eq!(scan.sessions.len(), 1);
    assert_eq!(scan.sessions[0].session_id, "codex-duplicate");
    assert_eq!(scan.sessions[0].title, "活动会话");

    // A rename recorded beside the transcript replaces the derived title
    // for the Codex record only; the summary keeps the first prompt.
    let renamed = scan_session_roots(
        &[(AppKind::Codex, active_root)],
        &HashMap::from([(
            "codex-duplicate".to_string(),
            "  部署线程（已重命名）  ".to_string(),
        )]),
    );
    assert_eq!(renamed.sessions[0].title, "部署线程（已重命名）");
    assert_eq!(renamed.sessions[0].summary, "活动会话");
}

#[test]
fn extracts_messages_only_when_a_record_has_a_role_and_content() {
    let temp = tempdir().expect("temp");
    let path = temp.path().join("session.jsonl");
    write(
        &path,
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"codex-123\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",\"content\":[{\"type\":\"input_text\",\"text\":\"你好\"}]}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"已完成\"}]}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"shell\"}}\n"
        ),
    );

    let messages = read_messages(&path).expect("read messages");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].content, "你好");
    assert_eq!(messages[1].role, "assistant");
}

#[test]
fn invalid_session_ids_are_not_made_resumable() {
    assert!(!parser::valid_session_id("session; shutdown"));
    assert!(!parser::valid_session_id(""));
    assert!(parser::valid_session_id(
        "019f4b74-c859-7e72-bb0c-9f83347954fb"
    ));
}

#[test]
fn metadata_records_beyond_the_line_cap_are_ignored() {
    let temp = tempdir().expect("temp");
    let mut content = String::new();
    for _ in 0..(parser::METADATA_LINE_LIMIT + 1) {
        content.push_str(
            "{\"type\":\"user\",\"message\":{\"role\":\"user\",\"content\":\"噪声\"}}\n",
        );
    }
    content.push_str(
        "{\"sessionId\":\"late-1\",\"cwd\":\"C:/work/late\",\"message\":{\"role\":\"user\",\"content\":\"很晚才出现\"}}\n",
    );
    write(&temp.path().join("late.jsonl"), &content);

    let scan = scan_session_roots(
        &[(AppKind::Claude, temp.path().to_path_buf())],
        &HashMap::new(),
    );
    assert!(
        scan.sessions.is_empty(),
        "会话 ID 在限界之后才出现时不应被收录"
    );
}

#[test]
fn removes_the_record_and_claude_sidecar_without_touching_siblings() {
    let temp = tempdir().expect("temp");
    let record = temp.path().join("session-1.jsonl");
    let sidecar = temp.path().join("session-1");
    write(&record, "{\"type\":\"user\"}\n");
    fs::create_dir_all(sidecar.join("parts")).expect("create sidecar");
    write(&sidecar.join("parts").join("chunk.bin"), "payload");
    let unrelated = temp.path().join("session-2.jsonl");
    write(&unrelated, "{\"type\":\"user\"}\n");

    remove_session_source(AppKind::Claude, &record).expect("delete claude record");
    assert!(!record.exists(), "会话记录应被删除");
    assert!(!sidecar.exists(), "同名附属目录应随会话删除");
    assert!(unrelated.exists(), "无关记录必须保留");

    let codex_record = temp.path().join("rollout-1.jsonl");
    let codex_sidecar = temp.path().join("rollout-1");
    write(&codex_record, "{\"type\":\"session_meta\"}\n");
    fs::create_dir_all(&codex_sidecar).expect("create codex sidecar");

    remove_session_source(AppKind::Codex, &codex_record).expect("delete codex record");
    assert!(!codex_record.exists(), "会话记录应被删除");
    assert!(codex_sidecar.exists(), "Codex 不清理同名目录");
}

#[test]
fn deleting_a_missing_record_reports_failure() {
    let temp = tempdir().expect("temp");
    let error = remove_session_source(AppKind::Codex, &temp.path().join("gone.jsonl"))
        .expect_err("missing record must fail");
    assert!(error.contains("无法删除会话记录"));
}

#[test]
fn batch_deletion_reports_each_request_and_removes_only_resolved_records() {
    let temp = tempdir().expect("temp");
    let root = temp.path().join("codex").join("sessions");
    for (file, id) in [("one.jsonl", "codex-one"), ("two.jsonl", "codex-two")] {
        write(
            &root.join(file),
            &format!(
                "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"cwd\":\"C:/work\",\"timestamp\":\"2026-08-01T10:00:00Z\"}}}}\n\
                 {{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":\"问题\"}}}}\n"
            ),
        );
    }
    let (sources, issues) =
        scan_session_source_roots(&[(AppKind::Codex, root.clone())], &HashMap::new());
    let request = |app, id: &str| SessionDeleteRequest {
        app,
        session_id: id.to_string(),
    };
    let outcomes = delete_resolved(
        temp.path(),
        &sources,
        &issues,
        &[
            request(AppKind::Codex, "codex-one"),
            request(AppKind::Codex, "codex-one"),
            request(AppKind::Codex, "codex-missing"),
            request(AppKind::Claude, "codex-two"),
            request(AppKind::Codex, "bad id;"),
        ],
    );

    assert_eq!(outcomes.len(), 4, "重复请求只回答一次");
    assert!(outcomes[0].deleted && outcomes[0].error.is_none());
    assert!(!outcomes[1].deleted);
    assert!(outcomes[1]
        .error
        .as_deref()
        .unwrap()
        .contains("找不到指定会话"));
    assert!(
        !outcomes[2].deleted,
        "同一 ID 属于另一客户端时不得跨客户端删除"
    );
    assert_eq!(outcomes[3].error.as_deref(), Some("会话 ID 无效"));
    assert!(!root.join("one.jsonl").exists());
    assert!(
        root.join("two.jsonl").exists(),
        "未解析到的请求不得影响其他记录"
    );
}

#[test]
fn resume_arguments_keep_the_client_command_and_id_separate() {
    assert_eq!(
        resume_arguments(AppKind::Codex, "codex-123"),
        ["codex", "resume", "codex-123"]
    );
    assert_eq!(
        resume_arguments(AppKind::Claude, "claude-456"),
        ["claude", "--resume", "claude-456"]
    );
}

#[test]
fn shell_resume_command_keeps_the_executable_and_arguments_separate() {
    assert_eq!(
        resume::shell_command(&["codex", "resume", "codex-123"]),
        "'codex' 'resume' 'codex-123'"
    );
    assert_eq!(
        resume::shell_command(&["claude", "--resume", "id-with-'quote"]),
        "'claude' '--resume' 'id-with-'\\''quote'"
    );
}
