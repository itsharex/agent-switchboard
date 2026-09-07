//! Renderer-view projection tests: the serialized shapes must never
//! leak credential material or machine paths.
#![cfg(test)]

use super::*;
use crate::commands::extensions::definitions::mcp_edit_view_of;
use asb_core::extensions::contracts::{McpDefinition, SkillDefinition, SourceRef};
use asb_core::extensions::edit::McpEditView;

fn definition(payload: ExtensionPayload) -> ExtensionDefinition {
    ExtensionDefinition {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: "ext-view".to_string(),
        name: "view".to_string(),
        revision: 1,
        created_at: "2026-09-06T00:00:00.000Z".to_string(),
        updated_at: "2026-09-06T00:00:00.000Z".to_string(),
        payload,
    }
}

#[test]
fn renderer_view_never_serializes_mcp_connection_material() {
    let mut headers = BTreeMap::new();
    headers.insert(
        "X-Private".to_string(),
        SecretValue::Plain {
            value: "private-header-value".to_string(),
        },
    );
    headers.insert(
        "Authorization".to_string(),
        SecretValue::SecretRef {
            reference: "credential-store-handle".to_string(),
        },
    );
    let item = extension_list_item(
        definition(ExtensionPayload::Mcp(
            asb_core::extensions::contracts::McpDefinition::Http {
                url: "https://private.example.test/mcp".to_string(),
                headers,
                bearer: Some(SecretValue::Plain {
                    value: "private-bearer-value".to_string(),
                }),
            },
        )),
        Vec::new(),
        Vec::new(),
        None,
    );

    let rendered = serde_json::to_string(&item).expect("view serializes");

    assert!(rendered.contains(REDACTED));
    assert!(rendered.contains(r#"mode":"stored"#));
    assert!(rendered.contains(r#"mode":"redacted"#));
    assert!(!rendered.contains("private.example.test"));
    assert!(!rendered.contains("private-header-value"));
    assert!(!rendered.contains("private-bearer-value"));
    assert!(!rendered.contains("credential-store-handle"));
}

#[test]
fn renderer_view_never_serializes_skill_source_identity_or_args() {
    let mut env = BTreeMap::new();
    env.insert(
        "MCP_TOKEN".to_string(),
        SecretValue::Plain {
            value: "argument-adjacent-value".to_string(),
        },
    );
    let mcp = extension_list_item(
        definition(ExtensionPayload::Mcp(
            asb_core::extensions::contracts::McpDefinition::Stdio {
                command: "mcp-server".to_string(),
                args: vec!["--api-key".to_string(), "private-argument".to_string()],
                env,
                codex_options: None,
            },
        )),
        Vec::new(),
        Vec::new(),
        None,
    );
    let skill = extension_list_item(
        definition(ExtensionPayload::Skill(SkillDefinition {
            content_digest: "a".repeat(64),
            manifest: SkillManifest {
                name: "safe-view".to_string(),
                description: Some("description".to_string()),
                license: None,
                allowed_tools: None,
                unparsed_keys: Vec::new(),
            },
            source: Some(SourceRef {
                source_id: "private-owner/private-repository".to_string(),
                subpath: "skills/safe-view".to_string(),
                ref_name: Some("main".to_string()),
                resolved_commit: Some("abcdef012345".to_string()),
            }),
            host_scoped: None,
            compatibility: Vec::new(),
            dependencies: Vec::new(),
        })),
        Vec::new(),
        Vec::new(),
        None,
    );

    let mcp_rendered = serde_json::to_string(&mcp).expect("view serializes");
    let skill_rendered = serde_json::to_string(&skill).expect("view serializes");

    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&mcp_rendered)
            .expect("JSON")
            .get("argumentCount")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );
    assert!(!mcp_rendered.contains("--api-key"));
    assert!(!mcp_rendered.contains("private-argument"));
    assert!(!mcp_rendered.contains("argument-adjacent-value"));
    assert!(!skill_rendered.contains("private-owner/private-repository"));
    assert!(!skill_rendered.contains("skills/safe-view"));
    assert!(!skill_rendered.contains(r#"refName"#));
    assert!(skill_rendered.contains("abcdef012345"));
}

#[test]
fn edit_view_carries_editable_fields_but_never_credential_handles() {
    let mut headers = BTreeMap::new();
    headers.insert(
        "X-Log-Level".to_string(),
        SecretValue::Plain {
            value: "debug".to_string(),
        },
    );
    headers.insert(
        "Authorization".to_string(),
        SecretValue::SecretRef {
            reference: "secret-handle-2".to_string(),
        },
    );
    let definition = definition(ExtensionPayload::Mcp(McpDefinition::Http {
        url: "https://mcp.example.test/v1".to_string(),
        headers,
        bearer: Some(SecretValue::SecretRef {
            reference: "secret-handle-3".to_string(),
        }),
    }));
    let view = mcp_edit_view_of(&definition).expect("mcp view builds");
    let rendered = serde_json::to_string(&view).expect("view serializes");
    // The URL and plain values are editable material and must prefill.
    assert!(rendered.contains("https://mcp.example.test/v1"));
    assert!(rendered.contains("debug"));
    // Credential-store handles and a reference field never appear.
    assert!(!rendered.contains("secret-handle"));
    assert!(!rendered.contains(r#""reference""#));
    assert!(view.name == "view" && view.revision == 1);
    let McpEditView::Http { bearer, .. } = &view.view else {
        panic!("expected http view");
    };
    assert_eq!(
        bearer.as_ref(),
        Some(&asb_core::extensions::SecretSlotView::SecretConfigured)
    );
}

#[test]
fn edit_view_refuses_skill_definitions() {
    let definition = definition(ExtensionPayload::Skill(SkillDefinition {
        content_digest: "a".repeat(64),
        manifest: SkillManifest {
            name: "safe-view".to_string(),
            description: Some("description".to_string()),
            license: None,
            allowed_tools: None,
            unparsed_keys: Vec::new(),
        },
        source: None,
        host_scoped: None,
        compatibility: Vec::new(),
        dependencies: Vec::new(),
    }));
    let error = mcp_edit_view_of(&definition).expect_err("skills have no MCP editor");
    assert_eq!(error.code, "extension-invalid");
}
