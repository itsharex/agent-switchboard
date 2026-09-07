//! Directory deploy/remove transaction tests: swap-in deploys with backups,
//! staleness rejection, and ownership-proving removal.

mod common;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::ExtensionTarget;
use asb_core::extensions::plan::{ExtensionPlan, PlanStep, PlannedFile};
use asb_switch::{apply_extension_plan, ExtensionApplyRequest, ExtensionError, FsIo};
use common::extensions::{apply, fixture, plan, Fixture};
use std::fs;
use std::path::{Path, PathBuf};

fn write_staging(root: &Path, files: &[(&str, &[u8])]) -> (PathBuf, Vec<PlannedFile>) {
    let staging = root.join("staging");
    fs::create_dir_all(staging.join("scripts")).unwrap();
    let mut planned = Vec::new();
    planned.push(PlannedFile {
        relative_path: "scripts".to_string(),
        digest: String::new(),
        mode: 0o755,
        size: 0,
    });
    for (relative, bytes) in files {
        let path = staging.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        planned.push(PlannedFile {
            relative_path: relative.to_string(),
            digest: {
                use sha2::Digest;
                sha2::Sha256::digest(bytes)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect()
            },
            mode: 0o644,
            size: bytes.len() as u64,
        });
    }
    (staging, planned)
}

fn directory_plan(
    fx: &Fixture,
    skill_root: &Path,
    staging: &Path,
    planned: &[PlannedFile],
    original_digest: Option<String>,
) -> ExtensionPlan {
    let deploy_name = "api-spec";
    plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![PlanStep::DirectoryDeploy {
            client: AppKind::Codex,
            target_dir: skill_root.join(deploy_name).to_string_lossy().to_string(),
            staging_dir: staging.to_string_lossy().to_string(),
            files: planned.to_vec(),
            original_digest,
            backup_dir: fx.backup_dir.to_string_lossy().to_string(),
        }],
    )
}

fn managed_file(relative: &str, bytes: &[u8], mode: u32) -> PlannedFile {
    use sha2::Digest;
    PlannedFile {
        relative_path: relative.to_string(),
        digest: sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        mode,
        size: bytes.len() as u64,
    }
}

fn directory_entry(relative: &str) -> PlannedFile {
    PlannedFile {
        relative_path: relative.to_string(),
        digest: String::new(),
        mode: 0o755,
        size: 0,
    }
}

#[test]
fn directory_deploys_into_an_empty_root_and_backs_up_nothing() {
    let fx = fixture("dir-fresh");
    let skill_root = fx._dir.path().join("home").join(".agents").join("skills");
    fs::create_dir_all(&skill_root).unwrap();
    let (staging, planned) = write_staging(
        &fx._dir.path(),
        &[("SKILL.md", b"---\nname: api-spec\ndescription: d\n---\n")],
    );

    let plan = directory_plan(&fx, &skill_root, &staging, &planned, None);
    let record = apply(&fx, &plan).unwrap();
    assert_eq!(
        record.resources[0].targets[0].outcome,
        asb_core::extensions::contracts::TargetOutcome::Applied
    );
    let deployed = skill_root.join("api-spec");
    assert_eq!(
        fs::read_to_string(deployed.join("SKILL.md")).unwrap(),
        "---\nname: api-spec\ndescription: d\n---\n"
    );
    // No sibling leftovers from the swap.
    let siblings: Vec<String> = fs::read_dir(&skill_root)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(siblings, vec!["api-spec".to_string()]);
}

#[test]
fn directory_update_swaps_content_and_keeps_the_previous_version_in_backup() {
    let fx = fixture("dir-update");
    let skill_root = fx._dir.path().join("home").join(".agents").join("skills");
    let deployed = skill_root.join("api-spec");
    fs::create_dir_all(&deployed).unwrap();
    fs::write(deployed.join("SKILL.md"), b"old content").unwrap();
    let original_digest = {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(b"SKILL.md".len().to_le_bytes());
        hasher.update(b"SKILL.md");
        hasher.update([b'F']);
        let file_hash = sha2::Sha256::digest(b"old content");
        hasher.update(file_hash);
        hasher.update(0o644_u32.to_le_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };

    let (staging, planned) = write_staging(&fx._dir.path(), &[("SKILL.md", b"new content")]);
    let plan = directory_plan(&fx, &skill_root, &staging, &planned, Some(original_digest));
    apply(&fx, &plan).unwrap();

    assert_eq!(fs::read(deployed.join("SKILL.md")).unwrap(), b"new content");
    // The previous version is recoverable from the operation backup.
    let backups: Vec<PathBuf> = fs::read_dir(&fx.backup_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(
        fs::read(backups[0].join("SKILL.md")).unwrap(),
        b"old content"
    );
}

#[test]
fn directory_update_rejects_externally_changed_targets() {
    let fx = fixture("dir-stale");
    let skill_root = fx._dir.path().join("home").join(".agents").join("skills");
    let deployed = skill_root.join("api-spec");
    fs::create_dir_all(&deployed).unwrap();
    fs::write(deployed.join("SKILL.md"), b"externally edited").unwrap();

    let (staging, planned) = write_staging(&fx._dir.path(), &[("SKILL.md", b"new content")]);
    let plan = directory_plan(
        &fx,
        &skill_root,
        &staging,
        &planned,
        Some("deadbeef".repeat(4)),
    );
    match apply(&fx, &plan) {
        Err(ExtensionError::PlanStale { .. }) => {}
        other => panic!("expected PlanStale, got {other:?}"),
    }
    assert_eq!(
        fs::read(deployed.join("SKILL.md")).unwrap(),
        b"externally edited"
    );
}

#[test]
fn directory_remove_proves_ownership_and_restores_from_backup_on_rollback() {
    let fx = fixture("dir-remove");
    let skill_root = fx._dir.path().join("home").join(".agents").join("skills");
    let deployed = skill_root.join("api-spec");
    fs::create_dir_all(&deployed).unwrap();
    fs::write(deployed.join("SKILL.md"), b"content").unwrap();

    // Wrong manifest: removal is refused.
    let wrong_plan = plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![PlanStep::DirectoryRemove {
            client: AppKind::Codex,
            target_dir: deployed.to_string_lossy().to_string(),
            expected_files: vec![managed_file("SKILL.md", b"different", 0o644)],
            expected_digest: "0".repeat(64),
            backup_dir: fx.backup_dir.to_string_lossy().to_string(),
        }],
    );
    assert!(matches!(
        apply(&fx, &wrong_plan),
        Err(ExtensionError::PlanStale { .. })
    ));
    assert!(deployed.exists());

    // Correct manifest with a failing commit: the removal is undone.
    let plan = plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![PlanStep::DirectoryRemove {
            client: AppKind::Codex,
            target_dir: deployed.to_string_lossy().to_string(),
            expected_files: vec![
                directory_entry("scripts"),
                managed_file("SKILL.md", b"content", 0o644),
            ],
            expected_digest: {
                use sha2::Digest;
                let mut hasher = sha2::Sha256::new();
                let mut entry = |path: &str, kind: u8, bytes: &[u8], mode: u32| {
                    hasher.update((path.len() as u64).to_le_bytes());
                    hasher.update(path.as_bytes());
                    hasher.update([kind]);
                    if !bytes.is_empty() {
                        hasher.update(sha2::Sha256::digest(bytes));
                    } else {
                        hasher.update([0u8; 32]);
                    }
                    hasher.update(mode.to_le_bytes());
                };
                entry("SKILL.md", b'F', b"content", 0o644);
                entry("scripts", b'D', b"", 0o755);
                hasher
                    .finalize()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            },
            backup_dir: fx.backup_dir.to_string_lossy().to_string(),
        }],
    );
    // The expected manifest includes the scripts directory; create it so
    // the walk matches, then apply with a failing commit.
    fs::create_dir_all(deployed.join("scripts")).unwrap();
    let error = apply_extension_plan(
        &FsIo,
        &ExtensionApplyRequest {
            operation_id: "op-1",
            plan: &plan,
            journal_dir: &fx.journal_dir,
            library_commit: None,
        },
        |_record, _steps| Err("store unreachable".to_string().into()),
    )
    .unwrap_err();
    match &error {
        ExtensionError::Failed { rollback, .. } => {
            assert_eq!(rollback.restored.len(), 1, "{rollback:?}");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert_eq!(fs::read(deployed.join("SKILL.md")).unwrap(), b"content");
}
