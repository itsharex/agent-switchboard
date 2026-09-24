use crate::{restore_projected, sha256_hex, FsIo, SwitchError};
use asb_core::{AppKind, BackupRecord};

#[test]
fn restore_guards_auth_even_when_the_backup_does_not_restore_auth() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("config.toml");
    let auth = directory.path().join("auth.json");
    let current = "model_provider = 'openai'\nmodel = 'current'";
    let restored = "model_provider = 'openai'\nmodel = 'before'";
    let backup_dir = directory.path().join("backups");
    std::fs::create_dir(&backup_dir).unwrap();
    let source = backup_dir.join("source.bak");
    std::fs::write(&source, restored).unwrap();
    std::fs::write(&target, current).unwrap();
    std::fs::write(&auth, "{\"updated\":true}").unwrap();
    let record = BackupRecord {
        id: "source".into(), app: AppKind::Codex, target_path: target.to_string_lossy().into_owned(),
        backup_path: source.to_string_lossy().into_owned(), created_at: "2026-09-22T00:00:00Z".into(),
        content_hash: sha256_hex(restored), target_existed: true, linked_backup_id: None, reason: "provider-projection".into(),
    };
    let expected = crate::RestoreExpectation { content_hash: sha256_hex(current), target_existed: true,
        auth: Some((sha256_hex("{}"), true)) };
    let result = restore_projected(&FsIo, &record, &target, None, Some(&expected), |_| panic!("must not commit"));
    assert!(matches!(result, Err(SwitchError::ExternalChange { .. })));
    assert_eq!(std::fs::read_to_string(target).unwrap(), current);
    assert_eq!(std::fs::read_to_string(auth).unwrap(), "{\"updated\":true}");
    assert_eq!(std::fs::read_dir(backup_dir).unwrap().count(), 1);
}

#[test]
fn confirmed_restore_rejects_a_later_edit_before_backups_or_writes() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("settings.json");
    let backup_dir = directory.path().join("backups");
    std::fs::create_dir(&backup_dir).unwrap();
    let source = backup_dir.join("source.bak");
    std::fs::write(&source, "{}").unwrap();
    std::fs::write(&target, "{\"model\":\"external\"}").unwrap();
    let record = BackupRecord {
        id: "source".into(), app: AppKind::Claude, target_path: target.to_string_lossy().into_owned(),
        backup_path: source.to_string_lossy().into_owned(), created_at: "2026-09-22T00:00:00Z".into(),
        content_hash: sha256_hex("{}"), target_existed: true, linked_backup_id: None, reason: "provider-projection".into(),
    };
    let expected = crate::RestoreExpectation { content_hash: sha256_hex("{\"model\":\"repaired\"}"), target_existed: true, auth: None };
    let result = restore_projected(&FsIo, &record, &target, None, Some(&expected), |_| panic!("must not commit"));
    assert!(matches!(result, Err(SwitchError::ExternalChange { .. })));
    assert_eq!(std::fs::read_to_string(target).unwrap(), "{\"model\":\"external\"}");
    assert_eq!(std::fs::read_dir(backup_dir).unwrap().count(), 1);
}

#[test]
fn codex_restore_validates_the_snapshot_and_projected_candidate_before_writing() {
    let current = "model_provider = 'openai'\nmodel = 'current'";
    let supported = "model_provider = 'openai'\nmodel = 'restored'";
    let retired = "model_provider = 'agent_switchboard'";
    for (source, candidate, accepted) in [
        (supported, supported, true),
        (retired, supported, false),
        (supported, retired, false),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("config.toml");
        let backup_dir = directory.path().join("backups");
        std::fs::create_dir(&backup_dir).unwrap();
        let backup_path = backup_dir.join("source.bak");
        std::fs::write(&target, current).unwrap();
        std::fs::write(&backup_path, source).unwrap();
        let record = BackupRecord {
            id: "source".into(),
            app: AppKind::Codex,
            target_path: target.to_string_lossy().into_owned(),
            backup_path: backup_path.to_string_lossy().into_owned(),
            created_at: "2026-09-20T00:00:00Z".into(),
            content_hash: sha256_hex(source),
            target_existed: true,
            linked_backup_id: None,
            reason: "provider-projection".into(),
        };
        let mut committed = false;
        let result = restore_projected(&FsIo, &record, &target, Some(candidate), None, |_| {
            committed = true;
            Ok(())
        });
        if accepted {
            assert!(result.is_ok());
        } else {
            assert!(matches!(result, Err(SwitchError::PlanRejected { .. })));
            assert_eq!(std::fs::read_dir(&backup_dir).unwrap().count(), 1);
        }
        assert_eq!(committed, accepted);
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            if accepted { candidate } else { current },
        );
        assert_eq!(std::fs::read_to_string(&backup_path).unwrap(), source);
    }
}
