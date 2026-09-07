use super::*;
use serde_json::json;

#[test]
fn portable_roundtrip_reports_missing_credential_slots() {
    isolated(
        "dev_api::extension_sandbox_tests::portable::portable_roundtrip_reports_missing_credential_slots",
        run_portable_workflow,
    );
}

fn run_portable_workflow(root: &Path) {
    let api = start_api(root);
    let skill = api.ok(
        "create_local_skill",
        json!({ "draft": { "name": "portable-helper", "description": "便携验证" } }),
    );
    let mcp = api.ok(
        "save_extension",
        json!({ "draft": { "name": "portable-docs", "payload": {
            "kind": "mcp", "transport": "stdio", "command": "npx",
            "args": ["-y", "portable-docs"],
            "env": { "TOKEN": { "mode": "plain", "value": "super-secret-value" } }
        } } }),
    );
    let skill_id = skill["id"].as_str().unwrap().to_string();
    let mcp_id = mcp["id"].as_str().unwrap().to_string();

    // Skill export carries the content and no machine paths.
    let skill_package = root.join("portable-helper.asb-ext.json");
    api.ok(
        "export_extension_portable",
        json!({ "definitionId": skill_id, "targetPath": skill_package.to_string_lossy() }),
    );
    let skill_text = read(&skill_package);
    let skill_value: Value = serde_json::from_str(&skill_text).unwrap();
    assert_eq!(skill_value["schemaVersion"], 1);
    assert!(
        skill_text.contains("SKILL.md"),
        "the package carries skill files"
    );
    assert!(
        !skill_text.contains(root.to_string_lossy().as_ref()),
        "no machine paths travel"
    );

    // MCP export keeps the stdio skeleton and env slot names only.
    let mcp_package = root.join("portable-docs.asb-ext.json");
    api.ok(
        "export_extension_portable",
        json!({ "definitionId": mcp_id, "targetPath": mcp_package.to_string_lossy() }),
    );
    let mcp_text = read(&mcp_package);
    let mcp_value: Value = serde_json::from_str(&mcp_text).unwrap();
    assert_eq!(mcp_value["envSlotNames"], json!(["TOKEN"]));
    assert!(
        !mcp_text.contains("super-secret-value"),
        "credential values never travel"
    );

    // Importing the MCP package reports the credential slot to re-bind.
    let report = api.ok(
        "import_extension_portable",
        json!({ "packagePath": mcp_package.to_string_lossy(), "confirmWrite": true }),
    );
    assert_eq!(report["missingEnvSlots"], json!(["TOKEN"]));
    assert_eq!(report["definition"]["name"], "portable-docs");
    assert_ne!(
        report["definition"]["id"].as_str().unwrap(),
        mcp_id,
        "the imported definition is a new library row"
    );

    // Importing the skill package creates an independent copy.
    let skill_report = api.ok(
        "import_extension_portable",
        json!({ "packagePath": skill_package.to_string_lossy(), "confirmWrite": true }),
    );
    assert_ne!(
        skill_report["definition"]["id"].as_str().unwrap(),
        skill_id,
        "the imported skill is a new library row"
    );

    // A remote MCP cannot be exported at all.
    let remote = api.ok(
        "save_extension",
        json!({ "draft": { "name": "remote-only", "payload": {
            "kind": "mcp", "transport": "http", "url": "https://example.internal/mcp"
        } } }),
    );
    api.rejected(
        "export_extension_portable",
        json!({ "definitionId": remote["id"], "targetPath": root.join("remote.asb-ext.json").to_string_lossy() }),
        "extension-invalid",
    );
}
