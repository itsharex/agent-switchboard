//! Shared fixtures for the extension transaction integration tests: the
//! journal/backup directory fixture and the plan builders.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::DocumentSyntax;
use asb_core::extensions::contracts::{
    DesiredState, ExtensionBinding, ExtensionTarget, EXTENSIONS_SCHEMA_VERSION,
};
use asb_core::extensions::plan::{ExtensionPlan, PlanStep, PlannedTarget};
use asb_switch::{apply_extension_plan, ExtensionApplyRequest, ExtensionError, FsIo};
use tempfile::TempDir;

pub struct Fixture {
    pub _dir: TempDir,
    pub journal_dir: PathBuf,
    pub backup_dir: PathBuf,
}

pub fn fixture(name: &str) -> Fixture {
    let dir = TempDir::new().expect("temp dir");
    let root = dir.path().join(name);
    let journal_dir = root.join("transactions").join("op-1");
    let backup_dir = root.join("backups").join("op-1");
    fs::create_dir_all(&journal_dir).unwrap();
    Fixture {
        _dir: dir,
        journal_dir,
        backup_dir,
    }
}

fn binding(target: &ExtensionTarget) -> ExtensionBinding {
    ExtensionBinding {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: "binding-1".to_string(),
        resource_id: "resource-1".to_string(),
        target: target.clone(),
        native_key: Some("docs".to_string()),
        deploy_name: None,
        desired: DesiredState::Enabled,
        locked_digest: None,
        last_applied_revision: None,
        updated_at: "2026-09-06T00:00:00Z".to_string(),
    }
}

pub fn document_step(path: &Path, expected_hash: Option<String>, rendered: &str) -> PlanStep {
    let existed = expected_hash.is_some();
    PlanStep::DocumentWrite {
        client: AppKind::Codex,
        path: path.to_string_lossy().to_string(),
        expected_content_hash: expected_hash,
        expected_existed: existed,
        rendered: rendered.to_string(),
        syntax: if path.to_string_lossy().ends_with(".toml") {
            DocumentSyntax::Toml
        } else {
            DocumentSyntax::Json
        },
        backup_dir: String::new(),
    }
}

pub fn plan(target: &ExtensionTarget, steps: Vec<PlanStep>) -> ExtensionPlan {
    ExtensionPlan {
        plan_id: "plan-1".to_string(),
        created_at: "2026-09-06T00:00:00Z".to_string(),
        preconditions: asb_core::extensions::plan::PlanPreconditions {
            expires_at: "2999-01-01T00:00:00Z".to_string(),
            generation: 0,
            secret_digests: Default::default(),
        },
        operations: vec![asb_core::extensions::plan::PlannedOperation {
            definition_id: "resource-1".to_string(),
            definition_revision: 1,
            operation: asb_core::extensions::contracts::PlanOperation::Install,
            targets: vec![PlannedTarget {
                binding_id: None,
                target: target.clone(),
                binding: binding(target),
                baseline: None,
                steps,
                warnings: Vec::new(),
                changes: Vec::new(),
            }],
        }],
    }
}

pub fn two_target_plan(first: &Path, second: &Path, backup_dir: &Path) -> ExtensionPlan {
    let mut plan = plan(
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        vec![document_step(
            first,
            sha(first),
            "model = \"gpt-5.2\"\n\n[mcp_servers.docs]\ncommand = \"npx\"\n",
        )],
    );
    for step in &mut plan.operations[0].targets[0].steps {
        if let PlanStep::DocumentWrite {
            backup_dir: dir, ..
        } = step
        {
            *dir = backup_dir.to_string_lossy().to_string();
        }
    }
    let mut second_step = document_step(second, sha(second), "{\n  \"kept\": true\n}\n");
    if let PlanStep::DocumentWrite {
        backup_dir: dir, ..
    } = &mut second_step
    {
        *dir = backup_dir.to_string_lossy().to_string();
    }
    plan.operations[0].targets.push(PlannedTarget {
        binding_id: None,
        target: ExtensionTarget::App {
            client: AppKind::Claude,
        },
        binding: binding(&ExtensionTarget::App {
            client: AppKind::Claude,
        }),
        baseline: None,
        steps: vec![second_step],
        warnings: Vec::new(),
        changes: Vec::new(),
    });
    plan
}

pub fn apply(
    fixture: &Fixture,
    plan: &ExtensionPlan,
) -> Result<asb_core::extensions::contracts::ExtensionOperationRecord, ExtensionError> {
    apply_extension_plan(
        &FsIo,
        &ExtensionApplyRequest {
            operation_id: "op-1",
            plan,
            journal_dir: &fixture.journal_dir,
            library_commit: None,
        },
        |_record, _steps| Ok(()),
    )
}

pub fn sha(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    use sha2::Digest;
    Some(
        sha2::Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}
