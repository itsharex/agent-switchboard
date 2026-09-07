use super::*;
use crate::contracts::AppKind;
use crate::extensions::contracts::{
    CodexServerOptions, DesiredState, ExtensionBinding, ExtensionDefinition, ExtensionPayload,
    ExtensionTarget, McpDefinition, SecretValue, SkillDefinition, SkillManifest, SourceRef,
    EXTENSIONS_SCHEMA_VERSION,
};
use std::collections::BTreeMap;

fn skill_definition() -> ExtensionDefinition {
    ExtensionDefinition {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: "skill-1".to_string(),
        name: "接口规范".to_string(),
        revision: 1,
        created_at: "2026-09-06T00:00:00Z".to_string(),
        updated_at: "2026-09-06T00:00:00Z".to_string(),
        payload: ExtensionPayload::Skill(SkillDefinition {
            content_digest: "a".repeat(64),
            manifest: SkillManifest {
                name: "api-spec".to_string(),
                description: Some("撰写接口规范".to_string()),
                license: None,
                allowed_tools: None,
                unparsed_keys: Vec::new(),
            },
            source: None,
            host_scoped: None,
            compatibility: Vec::new(),
            dependencies: Vec::new(),
        }),
    }
}

#[test]
fn definitions_with_valid_shapes_pass() {
    assert_eq!(validate_definition(&skill_definition()), Ok(()));
}

#[test]
fn portable_skills_require_a_description() {
    let mut definition = skill_definition();
    if let ExtensionPayload::Skill(skill) = &mut definition.payload {
        skill.manifest.description = None;
    }
    let error = validate_definition(&definition).unwrap_err();
    assert!(error.message.contains("description"));
    // Host-scoped imports follow the host rules instead.
    if let ExtensionPayload::Skill(skill) = &mut definition.payload {
        skill.host_scoped = Some(AppKind::Claude);
    }
    assert_eq!(validate_definition(&definition), Ok(()));
}

#[test]
fn skill_names_follow_the_portable_specification() {
    assert!(validate_skill_name("api-spec").is_ok());
    assert!(validate_skill_name("Api-Spec").is_err());
    assert!(validate_skill_name("-lead").is_err());
    assert!(validate_skill_name("double--hyphen").is_err());
    assert!(validate_skill_name(&"a".repeat(65)).is_err());
}

#[test]
fn server_keys_are_restricted() {
    assert!(validate_server_key("docs_search").is_ok());
    assert!(validate_server_key("docs.search").is_err());
    assert!(validate_server_key("").is_err());
}

#[test]
fn secret_shaped_plain_values_are_rejected() {
    let mut env = BTreeMap::new();
    env.insert(
        "TOKEN".to_string(),
        SecretValue::Plain {
            value: "sk-live-0123456789abcdef".to_string(),
        },
    );
    let error = validate_mcp_definition(&McpDefinition::Stdio {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "server".to_string()],
        env,
        codex_options: None,
    })
    .unwrap_err();
    assert!(error.message.contains("专用凭据接口"));
}

#[test]
fn env_references_must_be_valid_names() {
    let mut env = BTreeMap::new();
    env.insert(
        "TOKEN".to_string(),
        SecretValue::EnvRef {
            name: "NOT A NAME".to_string(),
        },
    );
    assert!(validate_mcp_definition(&McpDefinition::Stdio {
        command: "srv".to_string(),
        args: Vec::new(),
        env,
        codex_options: None,
    })
    .is_err());
}

#[test]
fn http_urls_must_be_http_or_https() {
    assert!(validate_mcp_definition(&McpDefinition::Http {
        url: "https://mcp.example.com/v1".to_string(),
        headers: BTreeMap::new(),
        bearer: None,
    })
    .is_ok());
    assert!(validate_mcp_definition(&McpDefinition::Http {
        url: "ftp://mcp.example.com".to_string(),
        headers: BTreeMap::new(),
        bearer: None,
    })
    .is_err());
}

#[test]
fn codex_rejects_project_private_targets() {
    let target = ExtensionTarget::ProjectPrivate {
        client: AppKind::Codex,
        project_id: "p-1".to_string(),
    };
    assert!(!target_scope_supported(&target));
    assert!(validate_target(&target).is_ok());
    let claude_target = ExtensionTarget::ProjectPrivate {
        client: AppKind::Claude,
        project_id: "p-1".to_string(),
    };
    assert!(target_scope_supported(&claude_target));
}

#[test]
fn skill_bindings_need_a_deploy_name_and_mcp_bindings_a_key() {
    let definition = skill_definition();
    let binding = ExtensionBinding {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: "b-1".to_string(),
        resource_id: definition.id.clone(),
        target: ExtensionTarget::App {
            client: AppKind::Codex,
        },
        native_key: None,
        deploy_name: Some("api-spec".to_string()),
        desired: DesiredState::Enabled,
        locked_digest: None,
        last_applied_revision: None,
        updated_at: "2026-09-06T00:00:00Z".to_string(),
    };
    assert_eq!(validate_binding(&definition, &binding), Ok(()));

    let mcp_definition = ExtensionDefinition {
        id: "mcp-1".to_string(),
        name: "文档检索".to_string(),
        payload: ExtensionPayload::Mcp(McpDefinition::Stdio {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "docs".to_string()],
            env: BTreeMap::new(),
            codex_options: Some(CodexServerOptions::default()),
        }),
        ..skill_definition()
    };
    let mcp_binding = ExtensionBinding {
        native_key: Some("docs".to_string()),
        deploy_name: None,
        resource_id: "mcp-1".to_string(),
        ..binding
    };
    assert_eq!(validate_binding(&mcp_definition, &mcp_binding), Ok(()));

    let bad = ExtensionBinding {
        native_key: None,
        deploy_name: Some("docs".to_string()),
        ..mcp_binding
    };
    assert!(validate_binding(&mcp_definition, &bad).is_err());
}

#[test]
fn locked_digests_are_skill_only_and_non_empty() {
    let definition = skill_definition();
    let binding = ExtensionBinding {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: "b-2".to_string(),
        resource_id: definition.id.clone(),
        target: ExtensionTarget::App {
            client: AppKind::Codex,
        },
        native_key: None,
        deploy_name: Some("api-spec".to_string()),
        desired: DesiredState::Enabled,
        locked_digest: Some("d".repeat(64)),
        last_applied_revision: None,
        updated_at: "2026-09-06T00:00:00Z".to_string(),
    };
    assert_eq!(validate_binding(&definition, &binding), Ok(()));

    let empty = ExtensionBinding {
        locked_digest: Some("  ".to_string()),
        ..binding.clone()
    };
    assert!(validate_binding(&definition, &empty).is_err());

    let mcp_definition = ExtensionDefinition {
        id: "mcp-1".to_string(),
        name: "文档检索".to_string(),
        payload: ExtensionPayload::Mcp(McpDefinition::Stdio {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "docs".to_string()],
            env: BTreeMap::new(),
            codex_options: Some(CodexServerOptions::default()),
        }),
        ..skill_definition()
    };
    let mcp_binding = ExtensionBinding {
        native_key: Some("docs".to_string()),
        deploy_name: None,
        resource_id: "mcp-1".to_string(),
        ..binding
    };
    assert!(validate_binding(&mcp_definition, &mcp_binding).is_err());
}

#[test]
fn content_paths_reject_traversal_and_reserved_names() {
    let entries = vec![
        ("SKILL.md".to_string(), ContentEntryKind::File, 10u64),
        ("scripts/run.sh".to_string(), ContentEntryKind::File, 10),
    ];
    assert!(validate_content_paths(&entries).is_ok());

    let traversal = vec![("../escape.md".to_string(), ContentEntryKind::File, 10u64)];
    assert!(validate_content_paths(&traversal).is_err());

    let reserved = vec![("CON/settings".to_string(), ContentEntryKind::File, 10u64)];
    assert!(validate_content_paths(&reserved).is_err());

    let absolute = vec![("/etc/passwd".to_string(), ContentEntryKind::File, 10u64)];
    assert!(validate_content_paths(&absolute).is_err());

    let collision = vec![
        ("Readme.md".to_string(), ContentEntryKind::File, 10u64),
        ("readme.MD".to_string(), ContentEntryKind::File, 10),
    ];
    assert!(validate_content_paths(&collision).is_err());

    let link = vec![("scripts".to_string(), ContentEntryKind::Link, 0u64)];
    assert!(validate_content_paths(&link).is_err());
}

#[test]
fn claude_only_transports_refuse_codex_clients() {
    for definition in [
        McpDefinition::ClaudeSse {
            url: "https://mcp.example.com/sse".to_string(),
            headers: BTreeMap::new(),
        },
        McpDefinition::ClaudeWs {
            url: "wss://mcp.example.com".to_string(),
            headers: BTreeMap::new(),
        },
    ] {
        assert!(!definition.supports_client(AppKind::Codex));
        assert!(definition.supports_client(AppKind::Claude));
    }
    assert!(McpDefinition::Http {
        url: "https://mcp.example.com".to_string(),
        headers: BTreeMap::new(),
        bearer: None,
    }
    .supports_client(AppKind::Codex));
}

#[test]
fn source_refs_survive_a_round_trip() {
    let reference = SourceRef {
        source_id: "src-1".to_string(),
        subpath: "skills/pdf".to_string(),
        ref_name: Some("main".to_string()),
        resolved_commit: Some("abc123".to_string()),
    };
    let json = serde_json::to_string(&reference).unwrap();
    assert_eq!(serde_json::from_str::<SourceRef>(&json).unwrap(), reference);
    // Unknown fields are rejected instead of preserved.
    assert!(serde_json::from_str::<SourceRef>(&format!(
        r#"{{"sourceId":"s","subpath":"","legacy":true}}"#
    ))
    .is_err());
}
