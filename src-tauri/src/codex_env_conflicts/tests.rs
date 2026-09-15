use super::*;
use asb_core::redact;
use std::path::PathBuf;

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("state");
    let rc = dir.path().join(".zshrc");
    std::fs::write(
        &rc,
        "# shell setup\nexport PATH=$PATH:/opt/bin\nexport OPENAI_BASE_URL=\"https://relay.internal/v1\"\nOPENAI_API_KEY='sk-codex-fixture-secret'\nexport openai_org_id=org-x\nexport ANTHROPIC_API_KEY=other-client\n",
    )
    .unwrap();
    (dir, root, rc)
}

#[test]
fn only_openai_variables_count_as_codex_conflicts() {
    assert!(matches("OPENAI_API_KEY"));
    assert!(matches("openai_base_url"));
    assert!(!matches("ANTHROPIC_API_KEY"));
    assert!(!matches("CODEX_HOME"), "CODEX_HOME 是路径覆盖，不是路由/凭据冲突");
    assert_eq!(
        parse_export("export OPENAI_BASE_URL=\"https://relay\""),
        Some(("OPENAI_BASE_URL".into(), "https://relay".into()))
    );
    assert_eq!(parse_export("# export OPENAI_API_KEY=x"), None);
}

#[test]
fn scanning_files_redacts_secrets_and_ignores_other_clients() {
    let (_dir, _root, rc) = fixture();
    let entries = shell_entries(&[rc.clone(), rc.with_file_name("missing")]);
    let conflicts: Vec<CodexEnvConflict> = entries.iter().map(Entry::conflict).collect();
    assert_eq!(conflicts.len(), 3);
    assert_eq!(conflicts[0].var_name, "OPENAI_BASE_URL");
    assert_eq!(conflicts[0].value_preview, "https://relay.internal/v1");
    assert_eq!(
        conflicts[0].source,
        CodexEnvSource::File {
            path: rc.to_string_lossy().into(),
            line: 3
        }
    );
    assert_eq!(conflicts[1].var_name, "OPENAI_API_KEY");
    assert_eq!(conflicts[1].value_preview, redact::REDACTED);
    assert_eq!(conflicts[2].var_name, "openai_org_id");
    let serialized = serde_json::to_string(&conflicts).unwrap();
    assert!(!serialized.contains("sk-codex-fixture-secret"));
    assert!(!serialized.contains("ANTHROPIC"));
}

#[test]
fn removal_requires_the_scan_revision_backs_up_full_values_and_restores_them() {
    let (_dir, root, rc) = fixture();
    let entries = shell_entries(&[rc.clone()]);
    let expected = revision(&entries);
    let selections = vec![
        CodexEnvSelection {
            var_name: "OPENAI_API_KEY".into(),
            source: entries[1].source.clone(),
        },
        CodexEnvSelection {
            var_name: "OPENAI_BASE_URL".into(),
            source: entries[0].source.clone(),
        },
    ];
    let stale = remove_selected(&root, entries.clone(), &selections, "stale");
    assert!(stale.unwrap_err().contains("重新扫描"));
    assert!(std::fs::read_to_string(&rc).unwrap().contains("OPENAI_API_KEY"));
    assert!(list_backups(&root).unwrap().is_empty());

    let backup = remove_selected(&root, entries.clone(), &selections, &expected).unwrap();
    assert_eq!(backup.entries.len(), 2);
    assert_eq!(backup.entries[0].value_preview, redact::REDACTED);
    let after = std::fs::read_to_string(&rc).unwrap();
    assert_eq!(
        after,
        "# shell setup\nexport PATH=$PATH:/opt/bin\nexport openai_org_id=org-x\nexport ANTHROPIC_API_KEY=other-client\n",
        "只删所选 OPENAI 行，其他客户端的变量原样保留"
    );
    let stored = std::fs::read_to_string(root.join(BACKUP_DIR).join(&backup.file_name)).unwrap();
    assert!(stored.contains("sk-codex-fixture-secret"));
    assert!(root.join(BACKUP_DIR).ends_with("backups/codex-env"));
    let listed = list_backups(&root).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].entries[0].value_preview, redact::REDACTED);

    assert_eq!(restore(&root, &backup.file_name).unwrap(), 2);
    let restored = std::fs::read_to_string(&rc).unwrap();
    assert!(restored.ends_with(
        "export OPENAI_API_KEY='sk-codex-fixture-secret'\nexport OPENAI_BASE_URL='https://relay.internal/v1'\n"
    ));
    assert_eq!(shell_entries(&[rc.clone()]).len(), 3);
    assert!(restore(&root, "../escape.json").unwrap_err().contains("无效"));
    assert!(restore(&root, "env-missing.json").is_err());
}

#[test]
fn a_line_that_changed_after_the_scan_is_not_removed() {
    let (_dir, root, rc) = fixture();
    let entries = shell_entries(&[rc.clone()]);
    let expected = revision(&entries);
    std::fs::write(&rc, "export OPENAI_BASE_URL=x\nexport OTHER=1\n").unwrap();
    let selections = vec![CodexEnvSelection {
        var_name: "OPENAI_API_KEY".into(),
        source: entries[1].source.clone(),
    }];
    let error = remove_selected(&root, entries, &selections, &expected).unwrap_err();
    assert!(error.contains("已不再定义"), "{error}");
    assert!(error.contains("备份已保存"));
    assert_eq!(
        std::fs::read_to_string(&rc).unwrap(),
        "export OPENAI_BASE_URL=x\nexport OTHER=1\n"
    );
}
