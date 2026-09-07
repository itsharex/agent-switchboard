#![cfg(test)]

use super::*;

use std::path::PathBuf;

use skills::observed_skill;

fn paths() -> DiscoveredPaths {
    DiscoveredPaths {
        home: PathBuf::from("C:/users/tester"),
        codex_home: None,
        claude_dir: None,
    }
}

fn binding(id: &str, target: ExtensionTarget, native_key: Option<&str>) -> ExtensionBinding {
    ExtensionBinding {
        schema_version: 2,
        id: id.to_string(),
        resource_id: "ext-1".to_string(),
        target,
        native_key: native_key.map(str::to_string),
        deploy_name: None,
        desired: asb_core::extensions::contracts::DesiredState::Enabled,
        locked_digest: None,
        last_applied_revision: Some(1),
        updated_at: "2026-09-06T00:00:00Z".to_string(),
    }
}

fn observed_mcp(client: AppKind, path: PathBuf) -> ObservedExtension {
    ObservedExtension {
        kind: ExtensionKind::Mcp,
        client,
        name: "docs".to_string(),
        description: None,
        origin: ObservedOrigin::UserRoot,
        path: path.to_string_lossy().to_string(),
        managed_binding_ids: Vec::new(),
        content_digest: None,
        transport: Some("HTTP".to_string()),
    }
}

#[test]
fn same_location_label_from_two_clients_stays_as_two_diagnostics() {
    let diagnostics = finalize_diagnostics(
        vec![
            DiagnosticSeed {
                code: DiagnosticCode::McpDocumentUnparsable,
                client: AppKind::Codex,
                subject: DiagnosticSubjectSeed::Location {
                    label: "MCP 配置文档".to_string(),
                    location_key: "codex-user-document".to_string(),
                    resource_kind: ExtensionKind::Mcp,
                },
                message: "无法解析".to_string(),
                remediation: DiagnosticRemediation::Manual {
                    reason: "请修复".to_string(),
                },
            },
            DiagnosticSeed {
                code: DiagnosticCode::McpDocumentUnparsable,
                client: AppKind::Claude,
                subject: DiagnosticSubjectSeed::Location {
                    label: "MCP 配置文档".to_string(),
                    location_key: "claude-user-document".to_string(),
                    resource_kind: ExtensionKind::Mcp,
                },
                message: "无法解析".to_string(),
                remediation: DiagnosticRemediation::Manual {
                    reason: "请修复".to_string(),
                },
            },
        ],
        &[],
        &mut |prefix| format!("{prefix}-id"),
    );
    assert_eq!(diagnostics.len(), 2);
}

#[test]
fn managed_markers_require_the_exact_client_document() {
    let paths = paths();
    let codex_user =
        crate::extensions::paths::user_mcp_document(AppKind::Codex, &paths.home, None, None);
    let claude_user =
        crate::extensions::paths::user_mcp_document(AppKind::Claude, &paths.home, None, None);
    let codex_project =
        crate::extensions::paths::codex_project_config_path(std::path::Path::new("C:/work/demo"));
    let attached = attach_bindings(
        vec![
            observed_mcp(AppKind::Codex, codex_user),
            observed_mcp(AppKind::Claude, claude_user),
            observed_mcp(AppKind::Codex, codex_project),
        ],
        &[binding(
            "bind-codex",
            ExtensionTarget::App {
                client: AppKind::Codex,
            },
            Some("docs"),
        )],
        &paths,
        &[],
    );

    assert_eq!(attached[0].managed_binding_ids, vec!["bind-codex"]);
    assert!(attached[1].managed_binding_ids.is_empty());
    assert!(attached[2].managed_binding_ids.is_empty());
}

#[test]
fn private_claude_observation_and_binding_keep_the_project_scope() {
    let paths = paths();
    let document =
        crate::extensions::paths::claude_user_json_path(&paths.home, paths.claude_dir.as_deref());
    let root = "C:/work/demo";
    let text = r#"{ "mcpServers": {
  "docs": { "type": "stdio", "command": "global" }
}, "projects": {
  "C:/work/demo": {
    "mcpServers": {
      "docs": { "type": "ws", "url": "wss://mcp.example.test/ws" }
    }
  }
} }"#;
    let mut observations = Vec::new();
    let mut private_diagnostics = Vec::new();
    append_claude_project_private_mcp_observations(
        &document,
        root,
        text,
        &mut observations,
        &mut private_diagnostics,
    );
    assert_eq!(observations.len(), 1);
    assert!(matches!(
        &observations[0].origin,
        ObservedOrigin::ProjectRoot { project_path } if project_path == root
    ));
    assert_eq!(observations[0].transport.as_deref(), Some("WebSocket"));

    let projects = vec![asb_core::extensions::contracts::ProjectRegistration {
        schema_version: 2,
        id: "proj-1".to_string(),
        root: root.to_string(),
        display_name: "demo".to_string(),
        registered_at: "2026-09-06T00:00:00Z".to_string(),
    }];
    let attached = attach_bindings(
        observations,
        &[binding(
            "bind-private",
            ExtensionTarget::ProjectPrivate {
                client: AppKind::Claude,
                project_id: "proj-1".to_string(),
            },
            Some("docs"),
        )],
        &paths,
        &projects,
    );
    assert_eq!(attached[0].managed_binding_ids, vec!["bind-private"]);
}

#[test]
fn pinned_skill_binding_stays_in_sync_across_definition_updates() {
    use asb_core::extensions::contracts::{SkillDefinition, SkillManifest};
    let dir = tempfile::TempDir::new().unwrap();
    let target_dir = dir.path().join("api-spec");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(
        target_dir.join("SKILL.md"),
        "---\nname: api-spec\ndescription: check\n---\nbody",
    )
    .unwrap();
    let entries = walk_skill_dir(&target_dir).unwrap();
    let deployed_digest = content_digest(&entries);
    let baseline = ManagedBaselineFile::new(
        "bind-skill",
        vec![ManagedBaseline::Directory {
            target_dir: target_dir.to_string_lossy().to_string(),
            original_existed: false,
            original_files: None,
            original_digest: None,
            last_files: Vec::new(),
            last_digest: deployed_digest.clone(),
            backup_reference: None,
        }],
    );
    let definition = ExtensionDefinition {
        schema_version: 2,
        id: "ext-skill-1".to_string(),
        name: "api-spec".to_string(),
        revision: 2,
        created_at: "2026-09-06T00:00:00Z".to_string(),
        updated_at: "2026-09-06T00:00:00Z".to_string(),
        payload: ExtensionPayload::Skill(SkillDefinition {
            content_digest: "new-digest".to_string(),
            manifest: SkillManifest {
                name: "api-spec".to_string(),
                description: None,
                license: None,
                allowed_tools: None,
                unparsed_keys: Vec::new(),
            },
            source: None,
            host_scoped: None,
            compatibility: Vec::new(),
            dependencies: Vec::new(),
        }),
    };
    let mut skill_binding = binding(
        "bind-skill",
        ExtensionTarget::App {
            client: AppKind::Codex,
        },
        None,
    );
    skill_binding.deploy_name = Some("api-spec".to_string());

    // Unpinned: the definition advanced past the last apply, so the
    // target reads as pending.
    assert_eq!(
        binding_file_state(&skill_binding, &definition, Some(&baseline), &[]),
        FileState::PendingApply
    );
    // Pinned to the deployed version: the definition revision stops
    // mattering and the target stays in sync.
    skill_binding.locked_digest = Some(deployed_digest);
    assert_eq!(
        binding_file_state(&skill_binding, &definition, Some(&baseline), &[]),
        FileState::InSync
    );
}

#[test]
fn skill_manifest_problems_are_three_distinguishable_diagnostics() {
    let dir = tempfile::TempDir::new().unwrap();

    // 1. SKILL.md missing entirely.
    let missing = dir.path().join("no-manifest");
    std::fs::create_dir_all(&missing).unwrap();
    std::fs::write(missing.join("README.md"), "not a manifest").unwrap();
    let observation = observed_skill(AppKind::Codex, ObservedOrigin::UserRoot, &missing).unwrap();
    assert_eq!(observation.observed.name, "no-manifest");
    assert!(observation.observed.description.is_none());
    assert_eq!(observation.observed.content_digest, None);
    let problem = observation.problem.expect("missing manifest is a problem");
    assert_eq!(problem.code, DiagnosticCode::SkillManifestMissing);

    // 2. SKILL.md exists without frontmatter — a different problem, and
    // never reported as "missing SKILL.md".
    let frontmatter_less = dir.path().join("plain-doc");
    std::fs::create_dir_all(&frontmatter_less).unwrap();
    std::fs::write(frontmatter_less.join("SKILL.md"), "# Just a document").unwrap();
    let observation =
        observed_skill(AppKind::Codex, ObservedOrigin::UserRoot, &frontmatter_less).unwrap();
    let problem = observation.problem.expect("frontmatter-less is a problem");
    assert_eq!(problem.code, DiagnosticCode::SkillFrontmatterMissing);
    assert!(observation.observed.description.is_none());

    // 3. Frontmatter unparsable — the failure lands in the diagnostic, not
    // in `description`.
    let broken = dir.path().join("broken-frontmatter");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("SKILL.md"), "---\ndescription: no name\n---\n").unwrap();
    let observation = observed_skill(AppKind::Codex, ObservedOrigin::UserRoot, &broken).unwrap();
    let problem = observation
        .problem
        .expect("broken frontmatter is a problem");
    assert_eq!(problem.code, DiagnosticCode::SkillFrontmatterInvalid);
    assert!(problem.message.contains("name"));
    assert!(observation.observed.description.is_none());
}

#[test]
fn mcp_collection_type_errors_become_diagnostics_not_empty_reads() {
    let mut observed = Vec::new();
    let mut diagnostics = Vec::new();
    // Codex: `mcp_servers` present but a scalar.
    append_mcp_observations(
        AppKind::Codex,
        std::path::Path::new("C:/cfg/config.toml"),
        ObservedOrigin::UserRoot,
        "model = \"x\"\nmcp_servers = \"broken\"\n",
        &mut observed,
        &mut diagnostics,
    );
    assert!(observed.is_empty());
    assert!(diagnostics
        .iter()
        .any(|seed| seed.code == DiagnosticCode::McpCollectionInvalid));

    // Claude: `mcpServers` present but an array.
    let mut diagnostics = Vec::new();
    append_mcp_observations(
        AppKind::Claude,
        std::path::Path::new("C:/cfg/claude.json"),
        ObservedOrigin::UserRoot,
        r#"{ "mcpServers": [] }"#,
        &mut observed,
        &mut diagnostics,
    );
    assert!(observed.is_empty());
    assert!(diagnostics
        .iter()
        .any(|seed| seed.code == DiagnosticCode::McpCollectionInvalid));

    // Entry-level problems keep their observation row and get typed codes.
    let mut diagnostics = Vec::new();
    append_mcp_observations(
        AppKind::Codex,
        std::path::Path::new("C:/cfg/config.toml"),
        ObservedOrigin::UserRoot,
        "[mcp_servers.both]\ncommand = \"npx\"\nurl = \"https://x\"\n",
        &mut observed,
        &mut diagnostics,
    );
    assert_eq!(observed.len(), 1, "the entry still becomes a row");
    assert!(diagnostics
        .iter()
        .any(|seed| seed.code == DiagnosticCode::McpTransportConflicting
            && matches!(
                seed.subject,
                DiagnosticSubjectSeed::Entry {
                    observation_index: 0
                }
            )));
}

#[test]
fn managed_binding_diagnostic_covers_missing_repairable_and_foreign_change() {
    use asb_core::extensions::contracts::{DesiredState, SkillDefinition, SkillManifest};

    let dir = tempfile::TempDir::new().unwrap();
    let target_dir = dir.path().join("api-spec");
    std::fs::create_dir_all(&target_dir).unwrap();
    std::fs::write(
        target_dir.join("SKILL.md"),
        "---\nname: api-spec\ndescription: check\n---\nbody",
    )
    .unwrap();
    std::fs::write(target_dir.join("extra.md"), "second file").unwrap();
    let entries = walk_skill_dir(&target_dir).unwrap();
    let deployed_digest = content_digest(&entries);
    let files: Vec<asb_core::extensions::contracts::ManagedFileEntry> = entries
        .iter()
        .map(|entry| asb_core::extensions::contracts::ManagedFileEntry {
            relative_path: entry.relative_path.clone(),
            digest: if entry.kind == asb_core::extensions::validate::ContentEntryKind::Dir {
                String::new()
            } else {
                let mut hasher = sha2::Sha256::new();
                use sha2::Digest;
                sha2::Digest::update(&mut hasher, &entry.bytes);
                sha2::Digest::finalize(hasher)
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect()
            },
            mode: entry.mode,
            size: entry.bytes.len() as u64,
        })
        .collect();
    let definition = ExtensionDefinition {
        schema_version: 2,
        id: "ext-skill-1".to_string(),
        name: "api-spec".to_string(),
        revision: 2,
        created_at: "2026-09-06T00:00:00Z".to_string(),
        updated_at: "2026-09-06T00:00:00Z".to_string(),
        payload: ExtensionPayload::Skill(SkillDefinition {
            content_digest: deployed_digest.clone(),
            manifest: SkillManifest {
                name: "api-spec".to_string(),
                description: Some("check".to_string()),
                license: None,
                allowed_tools: None,
                unparsed_keys: Vec::new(),
            },
            source: None,
            host_scoped: None,
            compatibility: Vec::new(),
            dependencies: Vec::new(),
        }),
    };
    let mut binding = binding(
        "bind-skill",
        ExtensionTarget::App {
            client: AppKind::Codex,
        },
        None,
    );
    binding.deploy_name = Some("api-spec".to_string());
    let baseline_with = |last_digest: String, last_files| {
        ManagedBaselineFile::new(
            "bind-skill",
            vec![ManagedBaseline::Directory {
                target_dir: target_dir.to_string_lossy().to_string(),
                original_existed: false,
                original_files: None,
                original_digest: None,
                last_files,
                last_digest,
                backup_reference: None,
            }],
        )
    };

    // In sync: nothing to report.
    assert!(managed_binding_diagnostic(
        &binding,
        &definition,
        Some(&baseline_with(deployed_digest.clone(), files.clone())),
        &[],
    )
    .is_none());

    // Some files deleted but the rest untouched: auto-repairable.
    std::fs::remove_file(target_dir.join("extra.md")).unwrap();
    let seed = managed_binding_diagnostic(
        &binding,
        &definition,
        Some(&baseline_with(deployed_digest.clone(), files.clone())),
        &[],
    )
    .expect("missing files are auto-repairable");
    assert_eq!(seed.code, DiagnosticCode::ManagedTargetMissing);
    assert!(matches!(
        seed.remediation,
        asb_core::extensions::diagnostics::DiagnosticRemediation::Auto { .. }
    ));

    // A modified surviving file turns the same situation into a manual
    // external change.
    std::fs::write(
        target_dir.join("SKILL.md"),
        "---\nname: api-spec\ndescription: TAMPERED\n---\nbody",
    )
    .unwrap();
    let seed = managed_binding_diagnostic(
        &binding,
        &definition,
        Some(&baseline_with(deployed_digest.clone(), files.clone())),
        &[],
    )
    .expect("modified content is a diagnostic");
    assert_eq!(seed.code, DiagnosticCode::ManagedTargetExternalChange);
    assert!(matches!(
        seed.remediation,
        asb_core::extensions::diagnostics::DiagnosticRemediation::Manual { .. }
    ));

    // The whole directory gone: auto-repairable again.
    std::fs::remove_dir_all(&target_dir).unwrap();
    let seed = managed_binding_diagnostic(
        &binding,
        &definition,
        Some(&baseline_with(deployed_digest.clone(), files.clone())),
        &[],
    )
    .expect("a missing managed directory is a diagnostic");
    assert_eq!(seed.code, DiagnosticCode::ManagedTargetMissing);

    // Disabled bindings are deliberately inactive and stay silent.
    let mut disabled = binding.clone();
    disabled.desired = DesiredState::Disabled;
    assert!(managed_binding_diagnostic(
        &disabled,
        &definition,
        Some(&baseline_with(deployed_digest, files)),
        &[],
    )
    .is_none());
}
