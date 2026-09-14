use super::*;
use serde_json::json;

// ------------------------------------------------------------------ takeover

#[test]
fn takeover_restores_native_state_on_removal() {
    isolated(
        "dev_api::extension_sandbox_tests::takeover::takeover_restores_native_state_on_removal",
        run_takeover_workflow,
    );
}

fn run_takeover_workflow(root: &Path) {
    let api = start_api(root);
    let codex_config = root.join("home/.codex/config.toml");
    write(
        &codex_config,
        "# host\nmodel = \"host-model\"\n\n[mcp_servers.docs]\ncommand = \"npx\"\nargs = [\"-y\", \"docs\"]\n",
    );
    let original_config = read(&codex_config);
    let skill_md = root.join("home/.agents/skills/native-helper/SKILL.md");
    write(
        &skill_md,
        "---\nname: native-helper\ndescription: 本机已有 Skill\n---\n\n# Native\n",
    );
    let original_skill = read(&skill_md);

    let discovered = api.ok("discover_extensions", json!({}));
    let observations = discovered["observations"].as_array().unwrap().clone();
    let mcp_obs = observations
        .iter()
        .find(|observation| observation["name"] == "docs" && observation["kind"] == "mcp")
        .expect("the native MCP entry is observed")
        .clone();
    let skill_obs = observations
        .iter()
        .find(|observation| {
            observation["name"] == "native-helper" && observation["kind"] == "skill"
        })
        .expect("the native skill directory is observed")
        .clone();
    assert_eq!(mcp_obs["managed"], false);
    assert_eq!(mcp_obs["origin"]["origin"], "userRoot");

    assert!(mcp_obs.get("path").is_none());
    assert!(skill_obs.get("path").is_none());
    // Taking over the MCP entry never writes the client document.
    let mcp_mutation = api.ok(
        "takeover_discovered_extension",
        json!({ "observationId": mcp_obs["observationId"], "confirmWrite": true }),
    );
    assert_eq!(
        read(&codex_config),
        original_config,
        "takeover never writes the client document"
    );
    let mcp_id = mcp_mutation["id"].as_str().unwrap().to_string();

    // Taking over the skill directory never writes the directory.
    let skill_mutation = api.ok(
        "takeover_discovered_extension",
        json!({ "observationId": skill_obs["observationId"], "confirmWrite": true }),
    );
    assert_eq!(
        read(&skill_md),
        original_skill,
        "takeover never writes the skill directory"
    );
    let skill_id = skill_mutation["id"].as_str().unwrap().to_string();

    // Both now report as managed by discovery.
    let rediscovered = api.ok("discover_extensions", json!({}));
    for name in ["docs", "native-helper"] {
        let observation = rediscovered["observations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|observation| observation["name"] == name)
            .expect("still observed after takeover");
        assert_eq!(observation["managed"], true, "{name} is managed");
    }

    // Removing both in one batch restores the takeover baseline: the native
    // entry stays in the document, the directory keeps its original files.
    let workspace = api.ok("list_extensions", json!({}));
    let binding_of = |definition_id: &str| -> String {
        workspace["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == definition_id)
            .expect("the taken-over definition is listed")["bindings"]
            .as_array()
            .unwrap()
            .first()
            .expect("the takeover created a binding")["id"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let view = api.ok(
        "prepare_extension_plan",
        json!({ "request": { "operations": [
            { "operation": "remove", "definitionId": mcp_id, "bindingId": binding_of(&mcp_id),
              "targets": [ { "scope": "app", "client": "codex" } ] },
            { "operation": "remove", "definitionId": skill_id, "bindingId": binding_of(&skill_id),
              "targets": [ { "scope": "app", "client": "codex" } ] }
        ] } }),
    );
    let applied = api.ok(
        "apply_extension_plan",
        json!({ "planId": view["planId"], "confirmWrite": true }),
    );
    assert!(applied["record"].is_object(), "the removal batch committed");
    assert_eq!(
        read(&codex_config),
        original_config,
        "removal restores the adopted native entry verbatim"
    );
    assert_eq!(
        read(&skill_md),
        original_skill,
        "removal restores the adopted directory content"
    );

    // After removal both are native again.
    let after = api.ok("discover_extensions", json!({}));
    for name in ["docs", "native-helper"] {
        let observation = after["observations"]
            .as_array()
            .unwrap()
            .iter()
            .find(|observation| observation["name"] == name)
            .expect("still present after removal");
        assert_eq!(observation["managed"], false, "{name} is unmanaged again");
    }
}

// ---------------------------------------------------------- portable packages
