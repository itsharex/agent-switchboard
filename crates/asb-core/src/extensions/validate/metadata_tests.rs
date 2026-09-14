use super::*;
use crate::extensions::contracts::{
    ExtensionDefinition, FieldEdit, McpMetadata, EXTENSIONS_SCHEMA_VERSION,
};
use crate::extensions::edit::apply_mcp_metadata;
use serde_json::{json, Value};

fn definition() -> Value {
    json!({ "schemaVersion": EXTENSIONS_SCHEMA_VERSION, "id": "ext-meta", "name": "docs", "revision": 1,
        "createdAt": "2026-09-12T00:00:00Z", "updatedAt": "2026-09-12T00:00:00Z",
        "kind": "mcp", "transport": "stdio", "command": "run" })
}

#[test]
fn existing_definitions_without_optional_metadata_remain_valid() {
    let stored: ExtensionDefinition = serde_json::from_value(definition()).unwrap();
    validate_definition(&stored).unwrap();
    assert!(stored.mcp_metadata.is_none());
    assert!(serde_json::to_value(stored)
        .unwrap()
        .get("mcpMetadata")
        .is_none());
}

#[test]
fn metadata_round_trips_in_the_definition_and_edits_are_explicit() {
    let metadata = json!({"displayName": "Docs Search", "description": "Search\nreference docs", "tags": ["docs", "search"],
        "homepage": "https://example.test", "docs": "https://example.test/docs"});
    let mut value = definition();
    value["mcpMetadata"] = metadata.clone();
    let stored: ExtensionDefinition = serde_json::from_value(value).unwrap();
    validate_definition(&stored).unwrap();
    assert_eq!(
        serde_json::to_value(&stored).unwrap()["mcpMetadata"],
        metadata
    );
    assert_eq!(
        apply_mcp_metadata(&stored.mcp_metadata, None).unwrap(),
        stored.mcp_metadata
    );
    assert!(
        apply_mcp_metadata(&stored.mcp_metadata, Some(&FieldEdit::Delete))
            .unwrap()
            .is_none()
    );
    let replacement = McpMetadata {
        display_name: Some("Updated".into()),
        ..McpMetadata::default()
    };
    assert_eq!(
        apply_mcp_metadata(
            &stored.mcp_metadata,
            Some(&FieldEdit::Replace {
                value: replacement.clone()
            })
        )
        .unwrap(),
        Some(replacement)
    );
}

#[test]
fn invalid_metadata_never_passes_whole_definition_validation() {
    for metadata in [
        json!({"displayName": " "}),
        json!({"description": "a\0b"}),
        json!({"tags": ["x", "x"]}),
        json!({"tags": ["bad\nlabel"]}),
        json!({"tags": vec!["tag"; 33]}),
        json!({"homepage": "file:///private"}),
        json!({"docs": "https://user:password@example.test"}),
    ] {
        let mut value = definition();
        value["mcpMetadata"] = metadata;
        let stored: ExtensionDefinition = serde_json::from_value(value).unwrap();
        assert_eq!(
            validate_definition(&stored).unwrap_err().field,
            "mcpMetadata"
        );
    }
    assert!(serde_json::from_value::<McpMetadata>(json!({"unknown": true})).is_err());
}

#[test]
fn skill_definitions_cannot_take_over_the_mcp_metadata_slot() {
    let mut value = definition();
    value.as_object_mut().unwrap().remove("transport");
    value.as_object_mut().unwrap().remove("command");
    value["kind"] = json!("skill");
    value["contentDigest"] = json!("a".repeat(64));
    value["manifest"] = json!({"name": "docs", "description": "Skill description"});
    value["mcpMetadata"] = json!({"displayName": "MCP only"});
    let stored: ExtensionDefinition = serde_json::from_value(value).unwrap();
    assert_eq!(
        validate_definition(&stored).unwrap_err().field,
        "mcpMetadata"
    );
}
