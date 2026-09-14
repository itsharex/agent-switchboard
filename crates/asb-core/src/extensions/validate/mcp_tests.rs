use super::validate_mcp_definition;
use crate::extensions::contracts::McpDefinition;
use serde_json::json;

#[test]
fn remote_header_validation_matches_the_editor_for_every_transport() {
    for transport in ["http", "claudeSse", "claudeWs"] {
        for headers in [
            json!({"Bad Name": {"mode": "plain", "value": "x"}}),
            json!({"X-Key": {"mode": "plain", "value": "a\nb"}}),
            json!({"X-Key": {"mode": "plain", "value": "a"}, "x-key": {"mode": "plain", "value": "b"}}),
        ] {
            let definition: McpDefinition = serde_json::from_value(json!({
                "transport": transport, "url": "https://mcp.test", "headers": headers,
            }))
            .unwrap();
            assert_eq!(
                validate_mcp_definition(&definition).unwrap_err().field,
                "headers"
            );
        }
    }
}

#[test]
fn http_bearer_and_authorization_must_not_compete_for_one_header() {
    let definition: McpDefinition =
        serde_json::from_value(json!({"transport": "http", "url": "https://mcp.test",
        "headers": {"aUtHoRiZaTiOn": {"mode": "secretRef", "reference": "header-handle"}},
        "bearer": {"mode": "secretRef", "reference": "bearer-handle"}}))
        .unwrap();
    assert_eq!(
        validate_mcp_definition(&definition).unwrap_err().field,
        "bearer"
    );
}

#[test]
fn codex_options_preserve_zero_false_and_reject_null_characters_in_cwd() {
    let mut value = json!({"transport": "stdio", "command": "run",
        "codexOptions": {"cwd": "/srv/docs", "startupTimeoutSec": 0, "toolTimeoutSec": 0, "required": false}});
    let definition: McpDefinition = serde_json::from_value(value.clone()).unwrap();
    validate_mcp_definition(&definition).unwrap();
    assert_eq!(
        serde_json::to_value(definition).unwrap()["codexOptions"],
        value["codexOptions"]
    );
    value["codexOptions"]["cwd"] = json!("a\0b");
    let invalid: McpDefinition = serde_json::from_value(value).unwrap();
    assert_eq!(
        validate_mcp_definition(&invalid).unwrap_err().field,
        "codexOptions"
    );
}
