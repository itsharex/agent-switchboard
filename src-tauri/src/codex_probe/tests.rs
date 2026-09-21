use super::*;
use super::questions::CUSTOM_QUESTION_ID;
use std::path::Path;
use std::sync::atomic::Ordering;

pub(super) fn write_rollout(root: &std::path::Path, id: &str, total: u64) -> std::path::PathBuf {
    let directory = root.join("sessions").join("t");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("rollout-{id}.jsonl"));
    let lines = [
        serde_json::json!({"type":"session_meta", "payload":{"id": id}}),
        serde_json::json!({"type":"event_msg", "payload":{"type":"token_count", "info":{
            "model":"gpt-test", "total_token_usage":{"input_tokens":120, "output_tokens":40,
                "cached_input_tokens":30, "reasoning_output_tokens":77, "total_tokens":total},
            "last_token_usage":{"input_tokens":120,"output_tokens":40,"cached_input_tokens":30}
        }}}),
    ];
    std::fs::write(&path, lines.map(|line| line.to_string()).join("\n") + "\n").unwrap();
    path
}

#[test]
fn answers_are_whole_nonnegative_integers_not_substrings() {
    use super::questions::answer_matches;
    assert!(answer_matches(" 45\n", "45"));
    assert!(answer_matches("00045", "45"));
    for answer in ["145", "45.5", "-45", "45 44", "not 45", "", "**45**"] {
        assert!(!answer_matches(answer, "45"), "{answer}");
    }
    let question = resolve_question(CUSTOM_QUESTION_ID, Some("题目".into()), Some("000".into())).unwrap();
    assert_eq!(question.expected_answer, "0");
}

#[test]
fn builtin_questions_have_unique_ids_and_pure_digit_answers() {
    let mut ids = Vec::new();
    for question in QUESTIONS.iter() {
        assert!(!question.id.is_empty());
        assert!(!question.label.is_empty());
        assert!(!question.prompt.is_empty());
        assert!(question
            .expected_answer
            .bytes()
            .all(|byte| byte.is_ascii_digit()));
        ids.push(question.id);
    }
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), QUESTIONS.len());
}

#[test]
fn resolve_question_validates_builtin_and_custom_selections() {
    let builtin = resolve_question(QUESTIONS[0].id, None, None).expect("builtin question");
    assert_eq!(builtin.id, QUESTIONS[0].id);
    assert_eq!(builtin.label, QUESTIONS[0].label);
    assert_eq!(builtin.expected_answer, QUESTIONS[0].expected_answer);

    let custom = resolve_question(
        CUSTOM_QUESTION_ID,
        Some("  题目文本 ".to_string()),
        Some(" 21 ".to_string()),
    )
    .expect("custom question");
    assert_eq!(custom.id, CUSTOM_QUESTION_ID);
    assert_eq!(custom.prompt, "题目文本");
    assert_eq!(custom.expected_answer, "21");

    assert!(resolve_question("unknown", None, None).is_err());
    assert!(resolve_question(CUSTOM_QUESTION_ID, None, None).is_err());
    assert!(resolve_question(CUSTOM_QUESTION_ID, Some("题目".into()), None).is_err());
    assert!(resolve_question(CUSTOM_QUESTION_ID, Some("题目".into()), Some("x".into())).is_err());
}

#[test]
fn registry_holds_one_worker_and_clears_it_when_finished() {
    let registry = ProbeRegistry::new();
    let cancel = registry.begin("batch-1").expect("first begin");
    assert!(registry.begin("batch-2").is_err());
    assert!(registry.cancel());
    // The second signal finds the flag already set.
    assert!(!registry.cancel());

    cancel.store(true, Ordering::SeqCst);
    registry.worker_finished("batch-1", None);
    // No worker left to cancel, and a new batch may take the slot.
    assert!(!registry.cancel());
    assert!(registry.begin("batch-2").is_ok());
}

#[test]
fn unsaved_results_stay_visible_until_flushed() {
    let registry = ProbeRegistry::new();
    let _cancel = registry.begin("batch-1").unwrap();
    let unsaved = UnsavedProbe {
        save_error: "模拟保存失败".into(),
        batch_id: "batch-1".to_string(),
        pending_runs: vec![ledger::ProbeRunRecord {
            seq: 2,
            status: ledger::ProbeRunStatus::Passed,
            ..Default::default()
        }],
        terminal: (ProbeBatchStatus::Failed, Some("检测结果保存失败".to_string())),
    };
    registry.worker_finished("batch-1", Some(unsaved));
    assert!(registry.unsaved("batch-1").is_some());

    assert!(registry.begin("batch-2").is_err());
    let temp = tempfile::tempdir().unwrap();
    let blocked_root = temp.path().join("not-a-directory");
    std::fs::write(&blocked_root, "occupied").unwrap();
    assert!(registry.retry_save(&ProbeLedger::new(&blocked_root)).is_err());
    assert!(registry.unsaved("batch-1").is_some());
    assert!(registry.begin("batch-2").is_err());
}

#[test]
fn fingerprint_changes_only_when_the_client_files_change() {
    let _guard = crate::test_client_paths::redirect_client_paths();
    let local = crate::local_state::LocalState::from_root(Path::new("state").to_path_buf());
    let target = local.target(asb_core::AppKind::Codex).unwrap();
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "model = \"gpt-5\"\n").unwrap();
    let first = snapshot::fingerprint(&local, None).unwrap();
    assert_eq!(snapshot::fingerprint(&local, None).unwrap(), first);
    std::fs::write(&target, "model = \"gpt-5.1\"\n").unwrap();
    assert_ne!(snapshot::fingerprint(&local, None).unwrap(), first);
    // The auth file participates: same config, different key material.
    std::fs::write(&target, "model = \"gpt-5\"\n").unwrap();
    assert_eq!(snapshot::fingerprint(&local, None).unwrap(), first);
    std::fs::write(target.with_file_name("auth.json"), "{\"OPENAI_API_KEY\":\"sk-x\"}").unwrap();
    assert_ne!(snapshot::fingerprint(&local, None).unwrap(), first);
}

#[test]
fn unidentified_configurations_stay_unlinked_instead_of_guessing() {
    let _guard = crate::test_client_paths::redirect_client_paths();
    let local = crate::local_state::LocalState::from_root(Path::new("state").to_path_buf());
    let target = local.target(asb_core::AppKind::Codex).unwrap();
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "model = \"gpt-5\"\n").unwrap();
    let record = snapshot::capture(&local, None).expect("snapshot");
    assert_eq!(record.profile_id, None);
    assert_eq!(record.profile_name, None);
    assert!(!record.fingerprint.is_empty());
}

#[test]
fn config_read_errors_are_not_hashed_as_missing_files() {
    let _guard = crate::test_client_paths::redirect_client_paths();
    let local = crate::local_state::LocalState::from_root(Path::new("state").to_path_buf());
    let target = local.target(asb_core::AppKind::Codex).unwrap();
    std::fs::create_dir_all(&target).unwrap();
    assert!(snapshot::fingerprint(&local, None).is_err());
}

#[test]
fn snapshot_reads_actual_model_and_removes_url_credentials() {
    let _guard = crate::test_client_paths::redirect_client_paths();
    let local = crate::local_state::LocalState::from_root(Path::new("state").to_path_buf());
    let target = local.target(asb_core::AppKind::Codex).unwrap();
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, r#"model = "actual-model"
model_reasoning_effort = "high"
openai_base_url = "https://user:secret@example.test/v1?key=secret#secret"
"#).unwrap();
    let config = snapshot::capture(&local, None).unwrap();
    assert_eq!(config.profile_model.as_deref(), Some("actual-model"));
    assert_eq!(config.reasoning_effort.as_deref(), Some("high"));
    assert_eq!(config.connection_identity.as_deref(), Some("https://example.test/v1"));
}

#[test]
fn pending_results_are_visible_in_both_views_and_retry_releases_the_slot() {
    let directory = tempfile::tempdir().unwrap();
    let local = crate::local_state::LocalState::from_root(directory.path().to_path_buf());
    let ledger = ProbeLedger::new(local.root());
    let registry = ProbeRegistry::new();
    registry.begin("pending").unwrap();
    ledger.insert_batch(&ledger::NewProbeBatch {
        id: "pending".into(), started_at: ledger::now_rfc3339(), planned_runs: 1,
        question: ledger::ProbeQuestionSnapshot { id: "custom".into(), label: "题".into(),
            text: "题干".into(), expected_answer: "45".into() },
        grading_version: "1".into(), cli_version: None, config: Default::default(),
    }).unwrap();
    ledger.start_run("pending", 1).unwrap();
    registry.worker_finished("pending", Some(UnsavedProbe {
        save_error: "模拟保存失败".into(),
        batch_id: "pending".into(), terminal: (ProbeBatchStatus::Completed, None),
        pending_runs: vec![ledger::ProbeRunRecord { seq: 1, status: ProbeRunStatus::Passed,
            final_answer: Some("45".into()), ..Default::default() }],
    }));
    let current = current_view(&local, &registry).unwrap().unwrap();
    let historical = batch_view(&local, &registry, "pending").unwrap().unwrap();
    for view in [current, historical] {
        assert!(view.persist_pending);
        assert_eq!(view.runs[0].final_answer.as_deref(), Some("45"));
    }
    retry_save(&local, &registry).unwrap();
    assert!(!batch_view(&local, &registry, "pending").unwrap().unwrap().persist_pending);
    assert!(registry.begin("next").is_ok());
}
