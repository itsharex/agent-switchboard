#![cfg(test)]

use super::*;

use std::path::PathBuf;

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
        diagnostics: Vec::new(),
    }
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
    append_claude_project_private_mcp_observations(&document, root, text, &mut observations);
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
