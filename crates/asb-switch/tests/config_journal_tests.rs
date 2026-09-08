mod common;

use asb_core::{test_support::CODEX_TOML, AppKind};
use asb_switch::{FsIo, PendingConfigWrite};
use std::{fs, panic::AssertUnwindSafe, path::Path};

fn interrupted(target: &Path, backups: &Path) -> PendingConfigWrite {
    let plan = common::codex_plan("journal", "https://upstream.example/v1", "new-model", "key");
    let preview = asb_switch::read_preview(&FsIo, target, &plan, "backups").unwrap();
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        asb_switch::execute(
            &FsIo,
            &asb_switch::SwitchRequest {
                target,
                plan: &plan,
                backup_dir: backups,
                expected_hash: &preview.content_hash,
                expected_rendered_hash: &preview.rendered_hash,
            },
            |_| panic!("simulated process stop after client replacement"),
        )
    }));
    assert!(result.is_err());
    // The process is still alive in this injected crash; release its lock explicitly.
    asb_switch::release(&FsIo, target).unwrap();
    asb_switch::pending_config_write(&FsIo, backups, AppKind::Codex)
        .unwrap()
        .unwrap()
}

#[test]
fn interrupted_write_blocks_new_writes_and_finishes_exact_snapshot() {
    let (_dir, target, backups) = common::setup(AppKind::Codex, CODEX_TOML);
    let auth = target.with_file_name("auth.json");
    fs::write(&auth, "account-owned-token").unwrap();
    let pending = interrupted(&target, &backups);
    assert_eq!(
        pending.backup.content_hash,
        asb_switch::sha256_hex(CODEX_TOML)
    );
    let plan = common::codex_plan("next", "https://next.example/v1", "next-model", "key");
    let preview = asb_switch::read_preview(&FsIo, &target, &plan, "backups").unwrap();
    let blocked = common::execute(
        &FsIo,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
    );
    assert!(blocked.unwrap_err().to_string().contains("未完成配置事务"));
    let mut committed = false;
    asb_switch::finish_config_recovery(
        &FsIo,
        &backups,
        &pending,
        &pending.after_hash,
        true,
        || {
            committed = true;
            Ok(())
        },
    )
    .unwrap();
    assert!(committed);
    assert!(
        asb_switch::pending_config_write(&FsIo, &backups, AppKind::Codex)
            .unwrap()
            .is_none()
    );
    assert_eq!(fs::read_to_string(auth).unwrap(), "account-owned-token");
}

#[test]
fn external_edit_is_blocked_until_exact_backup_is_explicitly_compensated() {
    let (_dir, target, backups) = common::setup(AppKind::Codex, CODEX_TOML);
    let pending = interrupted(&target, &backups);
    let edited = "not even valid TOML [";
    fs::write(&target, edited).unwrap();
    let result = asb_switch::finish_config_recovery(
        &FsIo,
        &backups,
        &pending,
        &pending.after_hash,
        true,
        || panic!("must not commit an unconfirmed external edit"),
    );
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&target).unwrap(), edited);
    let outcome =
        asb_switch::rollback_pending_config(&FsIo, &backups, &pending, || Ok(())).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), CODEX_TOML);
    assert_eq!(
        fs::read_to_string(outcome.pre_restore_backup.backup_path).unwrap(),
        edited
    );
    assert!(
        asb_switch::pending_config_write(&FsIo, &backups, AppKind::Codex)
            .unwrap()
            .is_none()
    );
}

#[test]
fn projected_restore_verifies_original_then_uses_rebuilt_endpoint() {
    let (_dir, target, backups) = common::setup(AppKind::Codex, CODEX_TOML);
    let pending = interrupted(&target, &backups);
    asb_switch::finish_config_recovery(
        &FsIo,
        &backups,
        &pending,
        &pending.after_hash,
        true,
        || Ok(()),
    )
    .unwrap();
    let rebuilt = CODEX_TOML.replace(":47821/", ":58901/");
    let outcome =
        asb_switch::restore_projected(&FsIo, &pending.backup, &target, Some(&rebuilt), |_| Ok(()))
            .unwrap();
    assert_eq!(outcome.restored_hash, asb_switch::sha256_hex(&rebuilt));
    assert_eq!(fs::read_to_string(&target).unwrap(), rebuilt);
    assert_eq!(
        fs::read_to_string(&pending.backup.backup_path).unwrap(),
        CODEX_TOML
    );
    fs::write(&pending.backup.backup_path, "changed backup").unwrap();
    assert!(asb_switch::restore_projected(
        &FsIo,
        &pending.backup,
        &target,
        Some(&rebuilt),
        |_| Ok(())
    )
    .is_err());
}
