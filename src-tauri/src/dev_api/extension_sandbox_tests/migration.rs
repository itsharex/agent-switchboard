use super::*;
use serde_json::json;

#[test]
fn v2_library_migrates_offline_before_any_command_runs() {
    isolated(
        "dev_api::extension_sandbox_tests::migration::v2_library_migrates_offline_before_any_command_runs",
        run_migration_workflow,
    );
}

fn run_migration_workflow(root: &Path) {
    // A v2 library from an earlier build: manifest, one definition, one
    // single-resource history record.
    let state = root.join("app-data/state/extensions");
    write(
        &state.join("manifest.json"),
        r#"{ "schemaVersion": 2, "generation": 4 }"#,
    );
    write(
        &state.join("definitions/ext-old.json"),
        r#"{
          "schemaVersion": 2,
          "id": "ext-old",
          "name": "legacy",
          "revision": 2,
          "createdAt": "2026-08-01T00:00:00Z",
          "updatedAt": "2026-08-01T00:00:00Z",
          "kind": "mcp",
          "transport": "stdio",
          "command": "npx"
        }"#,
    );
    write(
        &state.join("history/op-old.json"),
        r#"{
          "schemaVersion": 2,
          "record": {
            "schemaVersion": 2,
            "id": "op-old",
            "operation": "install",
            "definitionId": "ext-old",
            "definitionRevision": 1,
            "createdAt": "2026-08-01T00:00:00Z",
            "finishedAt": "2026-08-01T00:00:01Z",
            "targets": [ { "target": { "scope": "app", "client": "codex" },
                          "outcome": { "kind": "applied" } } ]
          },
          "completedSteps": [ {
            "target": { "scope": "app", "client": "codex" },
            "step": { "kind": "documentWritten", "path": "config.toml", "syntax": "toml",
                      "backupReference": null, "writtenHash": "h", "originalHash": null }
          } ],
          "preBaselines": [], "preBindings": [], "postBindings": [], "postBaselines": []
        }"#,
    );

    // Starting the API runs the one-shot migration before any command.
    let api = start_api(root);
    let manifest: Value = serde_json::from_str(&read(&state.join("manifest.json"))).unwrap();
    assert_eq!(manifest["schemaVersion"], 3);
    assert_eq!(
        manifest["generation"], 5,
        "the migration bumps the generation"
    );
    let history: Value = serde_json::from_str(&read(&state.join("history/op-old.json"))).unwrap();
    assert!(history["record"].get("definitionId").is_none());
    assert_eq!(history["record"]["resources"][0]["definitionId"], "ext-old");
    assert_eq!(
        history["completedSteps"][0]["resourceId"], "ext-old",
        "v2 steps are backfilled with the owning resource"
    );

    let workspace = api.ok("list_extensions", json!({}));
    let items = workspace["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "ext-old");
    assert_eq!(
        workspace["history"][0]["resources"][0]["definitionId"], "ext-old",
        "migrated history renders through the new shape"
    );
}
