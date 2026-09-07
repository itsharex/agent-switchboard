use super::*;

#[test]
fn combined_deployment_applies_and_restores_as_one_batch() {
    isolated(
        "dev_api::extension_sandbox_tests::deployment::combined_deployment_applies_and_restores_as_one_batch",
        run_combined_deployment,
    );
}

fn run_combined_deployment(root: &Path) {
    let api = start_api(root);
    let codex_config = root.join("home/.codex/config.toml");
    write(&codex_config, "# host\nmodel = \"host-model\"\n");
    let original_config = read(&codex_config);

    let skill = api.ok(
        "create_local_skill",
        json!({ "draft": { "name": "combo-helper", "description": "组合部署验证" } }),
    );
    let mcp = api.ok(
        "save_extension",
        json!({ "draft": { "name": "combo-docs", "payload": {
            "kind": "mcp", "transport": "stdio", "command": "npx", "args": ["-y", "combo-docs"]
        } } }),
    );
    api.ok(
        "update_skill_dependencies",
        json!({
            "definitionId": skill["id"],
            "update": {
                "expectedRevision": 1,
                "dependencies": [ { "name": "combo-docs", "resourceId": mcp["id"] } ]
            }
        }),
    );

    // The user picks the skill and its dependency MCP in one batch preview.
    let view = api.ok(
        "prepare_extension_plan",
        json!({ "request": { "operations": [
            { "operation": "install", "definitionId": skill["id"],
              "targets": [ { "scope": "app", "client": "codex" } ] },
            { "operation": "install", "definitionId": mcp["id"],
              "targets": [ { "scope": "app", "client": "codex" } ] }
        ] } }),
    );
    assert_eq!(
        view["operations"].as_array().unwrap().len(),
        2,
        "the preview groups one block per resource"
    );
    assert_eq!(view["operations"][0]["definitionId"], skill["id"]);
    assert_eq!(view["operations"][1]["definitionId"], mcp["id"]);

    // A duplicate operation on the same resource is rejected whole.
    api.rejected(
        "prepare_extension_plan",
        json!({ "request": { "operations": [
            { "operation": "install", "definitionId": skill["id"],
              "targets": [ { "scope": "app", "client": "codex" } ] },
            { "operation": "install", "definitionId": skill["id"],
              "targets": [ { "scope": "app", "client": "claude" } ] }
        ] } }),
        "extension-conflict",
    );

    let applied = api.ok(
        "apply_extension_plan",
        json!({ "planId": view["planId"], "confirmWrite": true }),
    );
    assert!(applied["record"].is_object(), "the batch committed");
    assert_eq!(applied["record"]["resources"].as_array().unwrap().len(), 2);

    let deployed_config = read(&codex_config);
    assert!(deployed_config.contains("combo-docs"));
    assert_ne!(deployed_config, original_config);
    let deployed_skill = root.join("home/.agents/skills/combo-helper/SKILL.md");
    assert!(deployed_skill.is_file(), "the skill directory deployed");

    // The workspace shows the dependency as bound on the shared target.
    let workspace = api.ok("list_extensions", json!({}));
    let skill_row = workspace["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == skill["id"])
        .unwrap()
        .clone();
    assert_eq!(
        skill_row["dependencyStates"][0]["state"], "bound",
        "a dependency deployed to the same target is bound"
    );

    // Restoring the batch reverses both resources in one preview.
    let restore_view = api.ok(
        "prepare_extension_restore",
        json!({ "operationId": applied["record"]["id"] }),
    );
    assert_eq!(
        restore_view["operations"].as_array().unwrap().len(),
        2,
        "restore replays the same resource grouping"
    );
    api.ok(
        "apply_extension_plan",
        json!({ "planId": restore_view["planId"], "confirmWrite": true }),
    );
    assert_eq!(read(&codex_config), original_config);
    assert!(
        !deployed_skill.exists(),
        "the managed skill directory is gone after restore"
    );
}

// ----------------------------------------------------------- schema migration
