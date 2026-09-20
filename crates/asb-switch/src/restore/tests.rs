use crate::{restore_projected, sha256_hex, FsIo, SwitchError};
use asb_core::{AppKind, BackupRecord};

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
        let result = restore_projected(&FsIo, &record, &target, Some(candidate), |_| {
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
