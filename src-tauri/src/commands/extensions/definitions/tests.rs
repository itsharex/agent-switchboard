use super::*;
use asb_core::extensions::contracts::{CodexServerOptions, McpDefinition};
use serde_json::{json, Value};
use tempfile::TempDir;

fn fixture() -> (TempDir, ExtensionStore, String) {
    let dir = TempDir::new().unwrap();
    let store = ExtensionStore::from_root(dir.path().join("state"));
    let draft = serde_json::from_value(json!({
        "name": "docs", "mcpMetadata": {"displayName": "Docs Search", "description": "Reference docs",
            "tags": ["docs", "search"], "homepage": "https://example.test", "docs": "https://example.test/docs"},
        "payload": {"kind": "mcp", "transport": "stdio", "command": "npx", "args": ["-y", " a b ", "", "two\nlines"],
            "env": {"TOKEN": {"mode": "secretRef", "reference": "never-expose-handle"}},
            "codexOptions": {"cwd": "/srv/docs", "startupTimeoutSec": 0, "toolTimeoutSec": 60, "required": false}}
    })).unwrap();
    let saved = save_definition(&store, draft).unwrap();
    (dir, store, saved.id)
}

fn request(value: Value) -> McpEditRequest {
    serde_json::from_value(value).unwrap()
}

#[test]
fn create_persists_metadata_and_projects_it_without_stored_secrets() {
    let (_dir, store, id) = fixture();
    let stored = store.get_definition(&id).unwrap().unwrap();
    let view = serde_json::to_value(mcp_edit_view_of(&stored).unwrap()).unwrap();
    assert_eq!(view["mcpMetadata"]["displayName"], "Docs Search");
    assert_eq!(
        view["codexOptions"],
        json!({"cwd": "/srv/docs", "startupTimeoutSec": 0, "toolTimeoutSec": 60, "required": false})
    );
    assert!(view.get("codex_options").is_none());
    assert_eq!(view["env"][0]["value"], json!({"mode": "secretConfigured"}));
    assert!(!view.to_string().contains("never-expose-handle"));
    let list = serde_json::to_value(extension_list_item(stored, vec![], vec![], None)).unwrap();
    assert_eq!(list["mcpMetadata"], view["mcpMetadata"]);
    assert_eq!(list["argumentCount"], 4);
    assert!(!list.to_string().contains("never-expose-handle"));
    assert_eq!(store.manifest().unwrap().generation, 1);
}

#[test]
fn metadata_only_updates_preserve_native_fields_and_do_not_rewrite_noops() {
    let (_dir, store, id) = fixture();
    let before = store.get_definition(&id).unwrap().unwrap();
    let updated = update_mcp(&store, &id, request(json!({"expectedRevision": 1,
        "mcpMetadata": {"action": "replace", "value": {"displayName": "Updated", "tags": ["docs"]}}}))).unwrap();
    assert_eq!(updated.revision, 2);
    let after = store.get_definition(&id).unwrap().unwrap();
    assert_eq!(after.payload, before.payload);
    assert_eq!(after.name, before.name);
    assert_eq!(
        after.mcp_metadata.unwrap().display_name.as_deref(),
        Some("Updated")
    );
    let unchanged = update_mcp(&store, &id, request(json!({"expectedRevision": 2}))).unwrap();
    assert_eq!(unchanged.revision, 2);
    assert_eq!(store.manifest().unwrap().generation, 2);
    update_mcp(
        &store,
        &id,
        request(json!({"expectedRevision": 2, "mcpMetadata": {"action": "delete"}})),
    )
    .unwrap();
    assert!(store
        .get_definition(&id)
        .unwrap()
        .unwrap()
        .mcp_metadata
        .is_none());
    assert_eq!(store.manifest().unwrap().generation, 3);
}

#[test]
fn field_edits_preserve_secrets_and_support_all_codex_options() {
    let (_dir, store, id) = fixture();
    update_mcp(&store, &id, request(json!({"expectedRevision": 1, "fields": {
        "command": {"action": "replace", "value": "docker"},
        "args": {"action": "replace", "value": [" a b ", "", "two\nlines"]},
        "codexOptions": {"action": "replace", "value": {"cwd": "/new", "startupTimeoutSec": 20, "toolTimeoutSec": 0, "required": true}}
    }}))).unwrap();
    let after = store.get_definition(&id).unwrap().unwrap();
    let ExtensionPayload::Mcp(McpDefinition::Stdio {
        env,
        args,
        codex_options,
        ..
    }) = after.payload
    else {
        panic!("stdio");
    };
    assert_eq!(
        env["TOKEN"],
        asb_core::extensions::contracts::SecretValue::SecretRef {
            reference: "never-expose-handle".into()
        }
    );
    assert_eq!(args, [" a b ", "", "two\nlines"]);
    assert_eq!(
        codex_options,
        Some(CodexServerOptions {
            cwd: Some("/new".into()),
            startup_timeout_sec: Some(20),
            tool_timeout_sec: Some(0),
            required: Some(true)
        })
    );
}

#[test]
fn metadata_and_name_changes_can_accompany_complete_transport_replacements() {
    for (transport, url) in [
        ("http", "https://mcp.test"),
        ("claudeSse", "https://mcp.test/sse"),
        ("claudeWs", "wss://mcp.test/ws"),
    ] {
        let (_dir, store, id) = fixture();
        update_mcp(&store, &id, request(json!({"expectedRevision": 1, "serverKey": "renamed",
            "mcpMetadata": {"action": "replace", "value": {"displayName": "Remote"}},
            "transport": {"transport": transport, "url": url, "headers": {"X-Key": {"mode": "secretRef", "reference": "new-only"}}}}))).unwrap();
        let after = store.get_definition(&id).unwrap().unwrap();
        assert_eq!(after.name, "renamed");
        assert_eq!(
            after.mcp_metadata.as_ref().unwrap().display_name.as_deref(),
            Some("Remote")
        );
        let json = serde_json::to_value(after).unwrap();
        assert_eq!(json["transport"], transport);
        assert!(json.get("env").is_none());
        assert!(json.get("codexOptions").is_none());
        assert!(!json.to_string().contains("never-expose-handle"));
    }
}

#[test]
fn invalid_or_stale_edits_leave_the_isolated_store_unchanged() {
    let (_dir, store, id) = fixture();
    let before = store.get_definition(&id).unwrap().unwrap();
    for edit in [
        json!({"expectedRevision": 0, "mcpMetadata": {"action": "delete"}}),
        json!({"expectedRevision": 1, "mcpMetadata": {"action": "replace", "value": {"docs": "file:///private"}}}),
        json!({"expectedRevision": 1, "fields": {"command": {"action": "replace", "value": " "}}}),
        json!({"expectedRevision": 1, "transport": {"transport": "http", "url": "https://test.invalid", "headers": {
            "X-Key": {"mode": "plain", "value": "one"}, "x-key": {"mode": "plain", "value": "two"}}}}),
    ] {
        assert!(update_mcp(&store, &id, request(edit)).is_err());
        assert_eq!(store.get_definition(&id).unwrap().unwrap(), before);
        assert_eq!(store.manifest().unwrap().generation, 1);
    }
}

#[test]
fn creation_rejects_invalid_metadata_before_persisting_anything() {
    let dir = TempDir::new().unwrap();
    let store = ExtensionStore::from_root(dir.path().join("state"));
    let draft = serde_json::from_value(
        json!({"name": "docs", "mcpMetadata": {"homepage": "javascript:bad"},
        "payload": {"kind": "mcp", "transport": "stdio", "command": "run"}}),
    )
    .unwrap();
    assert!(save_definition(&store, draft).is_err());
    assert!(store.list_definitions().unwrap().is_empty());
    assert_eq!(store.manifest().unwrap().generation, 0);
}
