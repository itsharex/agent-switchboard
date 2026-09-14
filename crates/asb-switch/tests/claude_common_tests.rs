//! Real writes and backups stay inside a temporary directory, never user configuration.
mod common;
use asb_core::{adapter, AppKind};
use asb_switch::{io::FsIo, read_preview};
use common::{claude_plan, execute, restore, setup};
use serde_json::{json, Value};
use std::fs;
#[test]
fn claude_common_survives_supplier_changes_and_is_recoverable_with_its_backup() {
    let initial =
        json!({"env":{"HOST":"keep"},"permissions":{"deny":["Bash(delete)"]}}).to_string();
    let (_dir, target, backups) = setup(AppKind::Claude, &initial);
    let mut first = claude_plan("first", "https://first.invalid", "first-model");
    first.client_settings=adapter::parse_client_settings(AppKind::Claude,r#"{"env":{"SHARED_FLAG":"1"},"permissions":{"allow":["Read"]},"spinnerTipsEnabled":false}"#).unwrap();
    let preview = read_preview(&FsIo, &target, &first, &backups.to_string_lossy()).unwrap();
    let result = execute(
        &FsIo,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &first,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
    )
    .unwrap();
    let after_first = fs::read_to_string(&target).unwrap();
    assert_eq!(
        fs::read_to_string(&result.backup.backup_path).unwrap(),
        initial
    );
    let mut second = claude_plan("second", "https://second.invalid", "second-model");
    second.client_settings = first.client_settings;
    let preview = read_preview(&FsIo, &target, &second, &backups.to_string_lossy()).unwrap();
    let result = execute(
        &FsIo,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &second,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
    )
    .unwrap();
    let value: Value = serde_json::from_str(&fs::read_to_string(&target).unwrap()).unwrap();
    assert_eq!(value["env"]["SHARED_FLAG"], "1");
    assert_eq!(value["env"]["HOST"], "keep");
    assert_eq!(value["permissions"]["deny"], json!(["Bash(delete)"]));
    assert_eq!(value["permissions"]["allow"], json!(["Read"]));
    restore(&FsIo, &result.backup, &target).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), after_first);
}
#[test]
fn stale_common_preview_never_overwrites_external_edits() {
    let (_dir, target, backups) = setup(AppKind::Claude, "{}");
    let mut plan = claude_plan("fixture", "https://fixture.invalid", "fixture-model");
    plan.client_settings =
        adapter::parse_client_settings(AppKind::Claude, r#"{"env":{"SHARED_FLAG":"1"}}"#).unwrap();
    let preview = read_preview(&FsIo, &target, &plan, &backups.to_string_lossy()).unwrap();
    fs::write(&target, r#"{"env":{"EXTERNAL":"keep"}}"#).unwrap();
    assert!(execute(
        &FsIo,
        &asb_switch::SwitchRequest {
            target: &target,
            plan: &plan,
            backup_dir: &backups,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash
        }
    )
    .is_err());
    assert_eq!(
        fs::read_to_string(&target).unwrap(),
        r#"{"env":{"EXTERNAL":"keep"}}"#
    );
}
