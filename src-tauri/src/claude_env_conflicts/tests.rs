use super::*;
use asb_core::redact;
use std::path::PathBuf;

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("state");
    let rc = dir.path().join(".zshrc");
    std::fs::write(
        &rc,
        "# shell setup\nexport PATH=$PATH:/opt/bin\nexport ANTHROPIC_BASE_URL=\"https://relay.internal\"\nANTHROPIC_API_KEY='sk-fixture-secret-value'\nexport anthropic_model=claude-x\nexport OTHER=1\n",
    )
    .unwrap();
    (dir, root, rc)
}

#[test]
fn shell_exports_are_parsed_with_quotes_stripped_and_comments_ignored() {
    assert_eq!(
        parse_export("export ANTHROPIC_BASE_URL=\"https://relay\""),
        Some(("ANTHROPIC_BASE_URL".into(), "https://relay".into()))
    );
    assert_eq!(
        parse_export("  ANTHROPIC_API_KEY='x y'  "),
        Some(("ANTHROPIC_API_KEY".into(), "x y".into()))
    );
    assert_eq!(parse_export("# export ANTHROPIC_API_KEY=x"), None);
    assert_eq!(parse_export("alias ll=ls -l"), None);
    assert_eq!(parse_export("if [ -f x ]; then"), None);
    assert!(matches("anthropic_model"));
    assert!(!matches("OPENAI_API_KEY"));
}

#[test]
fn scanning_files_redacts_secrets_and_keeps_line_numbers() {
    let (_dir, _root, rc) = fixture();
    let entries = shell_entries(&[rc.clone(), rc.with_file_name("missing")]);
    let conflicts: Vec<ClaudeEnvConflict> = entries.iter().map(Entry::conflict).collect();
    assert_eq!(conflicts.len(), 3);
    assert_eq!(conflicts[0].var_name, "ANTHROPIC_BASE_URL");
    assert_eq!(conflicts[0].value_preview, "https://relay.internal");
    assert_eq!(
        conflicts[0].source,
        ClaudeEnvSource::File {
            path: rc.to_string_lossy().into(),
            line: 3
        }
    );
    assert_eq!(conflicts[1].var_name, "ANTHROPIC_API_KEY");
    assert_eq!(conflicts[1].value_preview, redact::REDACTED);
    assert_eq!(conflicts[2].var_name, "anthropic_model");
    let serialized = serde_json::to_string(&conflicts).unwrap();
    assert!(!serialized.contains("sk-fixture-secret-value"));
}

#[test]
fn removal_requires_the_scan_revision_backs_up_full_values_and_restores_them() {
    let (_dir, root, rc) = fixture();
    let entries = shell_entries(&[rc.clone()]);
    let expected = revision(&entries);
    let selections = vec![
        ClaudeEnvSelection {
            var_name: "ANTHROPIC_API_KEY".into(),
            source: entries[1].source.clone(),
        },
        ClaudeEnvSelection {
            var_name: "ANTHROPIC_BASE_URL".into(),
            source: entries[0].source.clone(),
        },
    ];
    let stale = remove_selected(&root, entries.clone(), &selections, "stale");
    assert!(stale.unwrap_err().contains("重新扫描"));
    assert!(std::fs::read_to_string(&rc)
        .unwrap()
        .contains("ANTHROPIC_API_KEY"));
    assert!(list_backups(&root).unwrap().is_empty());

    let backup = remove_selected(&root, entries.clone(), &selections, &expected).unwrap();
    assert_eq!(backup.entries.len(), 2);
    assert_eq!(backup.entries[0].value_preview, redact::REDACTED);
    let after = std::fs::read_to_string(&rc).unwrap();
    assert_eq!(
        after,
        "# shell setup\nexport PATH=$PATH:/opt/bin\nexport anthropic_model=claude-x\nexport OTHER=1\n"
    );
    let stored = std::fs::read_to_string(root.join(BACKUP_DIR).join(&backup.file_name)).unwrap();
    assert!(stored.contains("sk-fixture-secret-value"));
    let listed = list_backups(&root).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].file_name, backup.file_name);
    assert_eq!(listed[0].entries[0].value_preview, redact::REDACTED);

    assert_eq!(restore(&root, &backup.file_name).unwrap(), 2);
    let restored = std::fs::read_to_string(&rc).unwrap();
    assert!(restored.ends_with(
        "export ANTHROPIC_API_KEY='sk-fixture-secret-value'\nexport ANTHROPIC_BASE_URL='https://relay.internal'\n"
    ));
    let rescanned = shell_entries(&[rc.clone()]);
    assert_eq!(rescanned.len(), 3);
    assert_eq!(rescanned[1].value, "sk-fixture-secret-value");
    assert!(restore(&root, "../escape.json")
        .unwrap_err()
        .contains("无效"));
    assert!(restore(&root, "env-missing.json").is_err());
}

#[test]
fn a_line_that_changed_after_the_scan_is_not_removed() {
    let (_dir, root, rc) = fixture();
    let entries = shell_entries(&[rc.clone()]);
    let expected = revision(&entries);
    std::fs::write(&rc, "export ANTHROPIC_BASE_URL=x\nexport OTHER=1\n").unwrap();
    let selections = vec![ClaudeEnvSelection {
        var_name: "ANTHROPIC_API_KEY".into(),
        source: entries[1].source.clone(),
    }];
    let error = remove_selected(&root, entries, &selections, &expected).unwrap_err();
    assert!(error.contains("已不再定义"), "{error}");
    assert!(error.contains("备份已保存"));
    assert_eq!(
        std::fs::read_to_string(&rc).unwrap(),
        "export ANTHROPIC_BASE_URL=x\nexport OTHER=1\n"
    );
}
