use super::*;

use std::collections::BTreeMap;

use crate::extensions::contracts::{CodexServerOptions, McpEditRequest, McpFieldEdits};
use crate::extensions::contracts::{FieldEdit, McpDefinition, SecretValue};
use crate::extensions::validate::validate_mcp_definition;

fn stdio_current() -> McpDefinition {
    let mut env = BTreeMap::new();
    env.insert(
        "TOKEN".to_string(),
        SecretValue::Plain {
            value: "never-rendered-sensitive".to_string(),
        },
    );
    env.insert(
        "KEEP_ME".to_string(),
        SecretValue::EnvRef {
            name: "KEEP_ME".to_string(),
        },
    );
    McpDefinition::Stdio {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "server".to_string()],
        env,
        codex_options: Some(CodexServerOptions {
            startup_timeout_sec: Some(30),
            ..CodexServerOptions::default()
        }),
    }
}

fn http_current() -> McpDefinition {
    let mut headers = BTreeMap::new();
    headers.insert(
        "X-Api-Key".to_string(),
        SecretValue::SecretRef {
            reference: "secret-1".to_string(),
        },
    );
    McpDefinition::Http {
        url: "https://mcp.example.test/v1".to_string(),
        headers,
        bearer: Some(SecretValue::SecretRef {
            reference: "secret-2".to_string(),
        }),
    }
}

#[test]
fn empty_request_keeps_everything_including_unshown_sensitive_values() {
    let request = McpEditRequest {
        expected_revision: 3,
        server_key: None,
        transport: None,
        fields: McpFieldEdits::default(),
    };
    let (name, next) = apply_mcp_edit("docs", &stdio_current(), &request).unwrap();
    assert_eq!(name, "docs");
    assert_eq!(next, stdio_current());
}

#[test]
fn replaces_renames_and_deletes_each_editable_field() {
    let mut env = BTreeMap::new();
    env.insert(
        "TOKEN".to_string(),
        FieldEdit::Replace {
            value: SecretValue::SecretRef {
                reference: "secret-new".to_string(),
            },
        },
    );
    env.insert("KEEP_ME".to_string(), FieldEdit::Delete);
    env.insert(
        "FRESH".to_string(),
        FieldEdit::Replace {
            value: SecretValue::Plain {
                value: "fresh-plain".to_string(),
            },
        },
    );
    let request = McpEditRequest {
        expected_revision: 1,
        server_key: Some("docs_v2".to_string()),
        transport: None,
        fields: McpFieldEdits {
            command: Some(FieldEdit::Replace {
                value: "docker".to_string(),
            }),
            args: Some(FieldEdit::Delete),
            env,
            url: None,
            headers: BTreeMap::new(),
            bearer: None,
            codex_options: Some(FieldEdit::Delete),
        },
    };
    let (name, next) = apply_mcp_edit("docs", &stdio_current(), &request).unwrap();
    assert_eq!(name, "docs_v2");
    match next {
        McpDefinition::Stdio {
            command,
            args,
            env,
            codex_options,
        } => {
            assert_eq!(command, "docker");
            assert!(args.is_empty());
            assert_eq!(env.len(), 2);
            assert!(matches!(
                env.get("TOKEN"),
                Some(SecretValue::SecretRef { reference }) if reference == "secret-new"
            ));
            // KEEP_ME was deleted; FRESH replaced into place.
            assert!(env.contains_key("FRESH"));
            assert!(codex_options.is_none());
        }
        other => panic!("unexpected variant {other:?}"),
    }
}

#[test]
fn http_fields_replace_and_delete() {
    let mut headers = BTreeMap::new();
    headers.insert(
        "X-Api-Key".to_string(),
        FieldEdit::Replace {
            value: SecretValue::SecretRef {
                reference: "secret-3".to_string(),
            },
        },
    );
    let request = McpEditRequest {
        expected_revision: 2,
        server_key: None,
        transport: None,
        fields: McpFieldEdits {
            url: Some(FieldEdit::Replace {
                value: "https://mcp.example.test/v2".to_string(),
            }),
            headers,
            bearer: Some(FieldEdit::Delete),
            ..McpFieldEdits::default()
        },
    };
    let (_, next) = apply_mcp_edit("api", &http_current(), &request).unwrap();
    match next {
        McpDefinition::Http {
            url,
            headers,
            bearer,
        } => {
            assert_eq!(url, "https://mcp.example.test/v2");
            assert_eq!(headers.len(), 1);
            assert!(bearer.is_none());
        }
        other => panic!("unexpected variant {other:?}"),
    }
}

#[test]
fn required_positions_cannot_be_deleted() {
    for request in [
        McpEditRequest {
            expected_revision: 1,
            server_key: None,
            transport: None,
            fields: McpFieldEdits {
                command: Some(FieldEdit::Delete),
                ..McpFieldEdits::default()
            },
        },
        McpEditRequest {
            expected_revision: 1,
            server_key: None,
            transport: None,
            fields: McpFieldEdits {
                url: Some(FieldEdit::Delete),
                ..McpFieldEdits::default()
            },
        },
    ] {
        let current = if request.fields.url.is_some() {
            &http_current()
        } else {
            &stdio_current()
        };
        let error = apply_mcp_edit("docs", current, &request).unwrap_err();
        assert!(error.message.contains("不能删除"), "{}", error.message);
    }
}

#[test]
fn inapplicable_field_edits_are_rejected_per_transport() {
    let request = McpEditRequest {
        expected_revision: 1,
        server_key: None,
        transport: None,
        fields: McpFieldEdits {
            url: Some(FieldEdit::Replace {
                value: "https://mcp.example.test".to_string(),
            }),
            ..McpFieldEdits::default()
        },
    };
    assert!(apply_mcp_edit("docs", &stdio_current(), &request).is_err());

    let mut env = BTreeMap::new();
    env.insert(
        "TOKEN".to_string(),
        FieldEdit::Replace {
            value: SecretValue::Plain {
                value: "v".to_string(),
            },
        },
    );
    let request = McpEditRequest {
        expected_revision: 1,
        server_key: None,
        transport: None,
        fields: McpFieldEdits {
            env,
            ..McpFieldEdits::default()
        },
    };
    assert!(apply_mcp_edit("api", &http_current(), &request).is_err());
}

#[test]
fn transport_switch_replaces_the_variant_and_rejects_field_edits() {
    let replacement = McpDefinition::ClaudeSse {
        url: "https://mcp.example.test/sse".to_string(),
        headers: BTreeMap::new(),
    };
    let request = McpEditRequest {
        expected_revision: 1,
        server_key: None,
        transport: Some(replacement.clone()),
        fields: McpFieldEdits::default(),
    };
    let (_, next) = apply_mcp_edit("docs", &stdio_current(), &request).unwrap();
    assert_eq!(next, replacement);

    let conflicting = McpEditRequest {
        fields: McpFieldEdits {
            command: Some(FieldEdit::Replace {
                value: "docker".to_string(),
            }),
            ..McpFieldEdits::default()
        },
        ..request.clone()
    };
    assert!(apply_mcp_edit("docs", &stdio_current(), &conflicting).is_err());
}

#[test]
fn server_keys_are_validated_on_change() {
    let request = McpEditRequest {
        expected_revision: 1,
        server_key: Some("not a key".to_string()),
        transport: None,
        fields: McpFieldEdits::default(),
    };
    let error = apply_mcp_edit("docs", &stdio_current(), &request).unwrap_err();
    assert_eq!(error.field, "nativeKey");
    // Re-sending the identical key is not a change and stays valid.
    let same = McpEditRequest {
        server_key: Some("docs".to_string()),
        ..request
    };
    assert!(apply_mcp_edit("docs", &stdio_current(), &same).is_ok());
}

#[test]
fn edit_requests_parse_strictly() {
    assert!(
        serde_json::from_str::<McpEditRequest>(r#"{"expectedRevision":1,"legacyField":true}"#)
            .is_err()
    );
    let parsed: McpEditRequest = serde_json::from_str(
            r#"{"expectedRevision":4,"serverKey":"docs2","fields":{"env":{"TOKEN":{"action":"delete"}}}}"#,
        )
        .unwrap();
    assert_eq!(parsed.server_key.as_deref(), Some("docs2"));
    assert!(matches!(
        parsed.fields.env.get("TOKEN"),
        Some(FieldEdit::Delete)
    ));
    // Unknown edit actions are rejected rather than ignored.
    assert!(serde_json::from_str::<McpEditRequest>(
        r#"{"expectedRevision":4,"fields":{"command":{"action":"touch","value":"x"}}}"#
    )
    .is_err());
}

#[test]
fn synthesized_definitions_must_pass_whole_validation() {
    // A replacement URL that is not http(s) fails the unified validator.
    let request = McpEditRequest {
        expected_revision: 1,
        server_key: None,
        transport: None,
        fields: McpFieldEdits {
            url: Some(FieldEdit::Replace {
                value: "ftp://mcp.example.test".to_string(),
            }),
            ..McpFieldEdits::default()
        },
    };
    assert!(apply_mcp_edit("api", &http_current(), &request).is_err());
    // A secret-shaped plain replacement is rejected the same way.
    let mut env = BTreeMap::new();
    env.insert(
        "TOKEN".to_string(),
        FieldEdit::Replace {
            value: SecretValue::Plain {
                value: "sk-live-0123456789abcdef".to_string(),
            },
        },
    );
    let request = McpEditRequest {
        expected_revision: 1,
        server_key: None,
        transport: None,
        fields: McpFieldEdits {
            env,
            ..McpFieldEdits::default()
        },
    };
    assert!(apply_mcp_edit("docs", &stdio_current(), &request).is_err());
}

#[test]
fn views_show_every_editable_field_but_never_credential_material() {
    let mut env = BTreeMap::new();
    env.insert(
        "LOG_LEVEL".to_string(),
        SecretValue::Plain {
            value: "debug".to_string(),
        },
    );
    env.insert(
        "TOKEN".to_string(),
        SecretValue::SecretRef {
            reference: "secret-view".to_string(),
        },
    );
    env.insert(
        "REGION".to_string(),
        SecretValue::EnvRef {
            name: "AWS_REGION".to_string(),
        },
    );
    let view = mcp_edit_view(&McpDefinition::Stdio {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "docs".to_string()],
        env,
        codex_options: Some(CodexServerOptions {
            cwd: Some("/srv".to_string()),
            ..CodexServerOptions::default()
        }),
    });
    let McpEditView::Stdio {
        command,
        args,
        env,
        codex_options,
    } = &view
    else {
        panic!("expected stdio view");
    };
    assert_eq!(command, "npx");
    assert_eq!(args.len(), 2);
    assert_eq!(
        codex_options.as_ref().and_then(|o| o.cwd.as_deref()),
        Some("/srv")
    );
    let token = env.iter().find(|slot| slot.name == "TOKEN").unwrap();
    assert_eq!(token.value, SecretSlotView::SecretConfigured);
    let json = serde_json::to_string(&view).unwrap();
    assert!(!json.contains("secret-view"));
    assert!(json.contains("debug"));
    assert!(json.contains("AWS_REGION"));
}

#[test]
fn http_and_claude_remote_views_carry_url_and_masked_secrets() {
    let view = mcp_edit_view(&http_current());
    let McpEditView::Http {
        url,
        headers,
        bearer,
    } = &view
    else {
        panic!("expected http view");
    };
    assert_eq!(url, "https://mcp.example.test/v1");
    assert_eq!(headers[0].value, SecretSlotView::SecretConfigured);
    assert_eq!(bearer.as_ref(), Some(&SecretSlotView::SecretConfigured));

    let sse = mcp_edit_view(&McpDefinition::ClaudeSse {
        url: "https://mcp.example.test/sse".to_string(),
        headers: BTreeMap::new(),
    });
    assert!(matches!(sse, McpEditView::ClaudeSse { .. }));
    let ws = mcp_edit_view(&McpDefinition::ClaudeWs {
        url: "wss://mcp.example.test/ws".to_string(),
        headers: BTreeMap::new(),
    });
    assert!(matches!(ws, McpEditView::ClaudeWs { .. }));
}

#[test]
fn ws_urls_validate_after_import_shapes() {
    // The strict importer produces ws/wss URLs for Claude ws entries;
    // the validator must accept what the importer emits.
    let ws = McpDefinition::ClaudeWs {
        url: "wss://mcp.example.test/ws".to_string(),
        headers: BTreeMap::new(),
    };
    assert_eq!(validate_mcp_definition(&ws), Ok(()));
    let ftp = McpDefinition::ClaudeWs {
        url: "ftp://mcp.example.test".to_string(),
        headers: BTreeMap::new(),
    };
    assert!(validate_mcp_definition(&ftp).is_err());
}
