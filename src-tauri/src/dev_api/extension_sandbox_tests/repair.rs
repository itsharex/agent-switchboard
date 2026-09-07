use super::*;

fn diagnostic_of<'a>(discovery: &'a Value, code: &str) -> Option<Value> {
    discovery["diagnostics"]
        .as_array()?
        .iter()
        .find(|diagnostic| diagnostic["code"] == code)
        .cloned()
}

/// The full repair lifecycle of one managed Skill: delete the deployed
/// directory, repair it from the discovery scan, and verify the applied
/// revision and content version never moved; the repair itself is restorable.
#[test]
fn missing_managed_skill_repairs_to_the_last_applied_version() {
    isolated(
        "dev_api::extension_sandbox_tests::repair::missing_managed_skill_repairs_to_the_last_applied_version",
        run_missing_skill_repair,
    );
}

fn run_missing_skill_repair(root: &Path) {
    let api = start_api(root);
    let skill = api.ok(
        "create_local_skill",
        json!({ "draft": { "name": "repair-helper", "description": "修复链路验证" } }),
    );
    let deployed = root.join("home/.agents/skills/repair-helper");
    let deployed_manifest = deployed.join("SKILL.md");

    let view = api.ok(
        "prepare_extension_plan",
        json!({ "request": { "operations": [
            { "operation": "install", "definitionId": skill["id"],
              "targets": [ { "scope": "app", "client": "codex" } ] }
        ] } }),
    );
    api.ok(
        "apply_extension_plan",
        json!({ "planId": view["planId"], "confirmWrite": true }),
    );
    assert!(deployed_manifest.is_file(), "the skill deployed");
    let original_text = read(&deployed_manifest);

    let workspace = api.ok("list_extensions", json!({}));
    let binding_id = workspace["items"][0]["bindings"][0]["id"].clone();
    let applied_revision = workspace["items"][0]["bindings"][0]["lastAppliedRevision"].clone();

    // Delete the deployed directory behind the application's back and scan.
    fs::remove_dir_all(&deployed).unwrap();
    let discovery = api.ok("discover_extensions", json!({}));
    assert!(
        discovery["scanId"].is_string(),
        "the scan carries an identity"
    );
    assert!(
        discovery["scannedAt"].is_string(),
        "the scan carries its time"
    );
    let diagnostic = diagnostic_of(&discovery, "managedTargetMissing")
        .expect("the missing managed directory is diagnosed");
    assert_eq!(diagnostic["subject"]["kind"], "managedBinding");
    assert_eq!(diagnostic["subject"]["bindingId"], binding_id);
    assert_eq!(diagnostic["remediation"]["kind"], "auto");
    assert_eq!(discovery["diagnostics"].as_array().unwrap().len(), 1);

    // A repair against a stale or unknown scan is rejected outright.
    api.rejected(
        "prepare_extension_repair",
        json!({ "request": { "scanId": "scan-does-not-exist",
             "diagnosticIds": [diagnostic["id"]] } }),
        "extension-stale-scan",
    );

    // The real repair previews through the shared plan view and applies.
    let repair = api.ok(
        "prepare_extension_repair",
        json!({ "request": { "scanId": discovery["scanId"],
             "diagnosticIds": [diagnostic["id"]] } }),
    );
    assert_eq!(repair["operations"][0]["operation"], "repair");
    assert_eq!(repair["operations"][0]["definitionId"], skill["id"]);
    let outcome = api.ok(
        "apply_extension_plan",
        json!({ "planId": repair["planId"], "confirmWrite": true }),
    );
    assert!(outcome["record"].is_object(), "the repair committed");
    assert_eq!(outcome["record"]["resources"][0]["operation"], "repair");

    // The directory is back, byte-identical, at the same applied revision.
    assert_eq!(read(&deployed_manifest), original_text);
    let workspace = api.ok("list_extensions", json!({}));
    assert_eq!(
        workspace["items"][0]["bindings"][0]["lastAppliedRevision"], applied_revision,
        "a repair never advances lastAppliedRevision"
    );

    // A fresh scan confirms the diagnostic disappeared, and the stale
    // diagnostic id no longer belongs to the latest scan.
    let discovery = api.ok("discover_extensions", json!({}));
    assert!(
        diagnostic_of(&discovery, "managedTargetMissing").is_none(),
        "the repaired object is no longer diagnosed"
    );
    api.rejected(
        "prepare_extension_repair",
        json!({ "request": { "scanId": discovery["scanId"],
             "diagnosticIds": [diagnostic["id"]] } }),
        "extension-stale-scan",
    );

    // History restore replays the repair's exact inverse: the directory is
    // removed again without inventing new state.
    let restore = api.ok(
        "prepare_extension_restore",
        json!({ "operationId": outcome["record"]["id"] }),
    );
    api.ok(
        "apply_extension_plan",
        json!({ "planId": restore["planId"], "confirmWrite": true }),
    );
    assert!(
        !deployed.exists(),
        "restore returned to the pre-repair state"
    );
}

/// A modified surviving file is an external change: the diagnostic demands
/// manual work and the repair command refuses to fake it.
#[test]
fn externally_changed_managed_skill_is_manual_and_repair_refuses_it() {
    isolated(
        "dev_api::extension_sandbox_tests::repair::externally_changed_managed_skill_is_manual_and_repair_refuses_it",
        run_external_change_repair_refusal,
    );
}

fn run_external_change_repair_refusal(root: &Path) {
    let api = start_api(root);
    let skill = api.ok(
        "create_local_skill",
        json!({ "draft": { "name": "tamper-target", "description": "冲突验证" } }),
    );
    let view = api.ok(
        "prepare_extension_plan",
        json!({ "request": { "operations": [
            { "operation": "install", "definitionId": skill["id"],
              "targets": [ { "scope": "app", "client": "codex" } ] }
        ] } }),
    );
    api.ok(
        "apply_extension_plan",
        json!({ "planId": view["planId"], "confirmWrite": true }),
    );
    let manifest = root.join("home/.agents/skills/tamper-target/SKILL.md");
    fs::write(
        &manifest,
        "---\nname: tamper-target\ndescription: TAMPERED\n---\n",
    )
    .unwrap();

    let discovery = api.ok("discover_extensions", json!({}));
    let diagnostic = diagnostic_of(&discovery, "managedTargetExternalChange")
        .expect("the modified file is an external change");
    assert_eq!(diagnostic["remediation"]["kind"], "manual");
    api.rejected(
        "prepare_extension_repair",
        json!({ "request": { "scanId": discovery["scanId"],
             "diagnosticIds": [diagnostic["id"]] } }),
        "extension-invalid",
    );
}

/// Managed MCP entries deleted from an otherwise intact document restore
/// verbatim from their baseline values; several repairs to one document
/// fold into a single document patch in one transaction.
#[test]
fn missing_managed_mcp_entries_restore_in_one_document_patch() {
    isolated(
        "dev_api::extension_sandbox_tests::repair::missing_managed_mcp_entries_restore_in_one_document_patch",
        run_mcp_entry_repair,
    );
}

#[test]
fn private_mcp_recreated_after_scan_blocks_repair() {
    isolated(
        "dev_api::extension_sandbox_tests::repair::private_mcp_recreated_after_scan_blocks_repair",
        run_private_mcp_repair_conflict,
    );
}

fn run_private_mcp_repair_conflict(root: &Path) {
    let api = start_api(root);
    let project_dir = root.join("private-repair-project");
    fs::create_dir_all(&project_dir).unwrap();
    let project = api.ok(
        "register_project",
        json!({ "root": project_dir.to_string_lossy() }),
    );
    let project_root = project["root"].as_str().unwrap();
    let config = root.join("home/.claude.json");
    let mut document = json!({ "projects": {} });
    document["projects"][project_root] = json!({
        "mcpServers": { "docs": { "type": "stdio", "command": "original" } }
    });
    write(&config, &document.to_string());

    let scan = api.ok("discover_extensions", json!({}));
    let observation = scan["observations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == "docs")
        .unwrap();
    api.ok(
        "takeover_discovered_extension",
        json!({ "observationId": observation["observationId"], "confirmWrite": true }),
    );

    document["projects"][project_root]["mcpServers"] = json!({});
    write(&config, &document.to_string());
    let scan = api.ok("discover_extensions", json!({}));
    let diagnostic = diagnostic_of(&scan, "managedEntryMissing").unwrap();

    document["projects"][project_root]["mcpServers"]["docs"] =
        json!({ "type": "stdio", "command": "external-new" });
    write(&config, &document.to_string());
    api.rejected(
        "prepare_extension_repair",
        json!({ "request": {
            "scanId": scan["scanId"],
            "diagnosticIds": [diagnostic["id"]]
        } }),
        "extension-conflict",
    );

    let restored: Value = serde_json::from_str(&read(&config)).unwrap();
    assert_eq!(
        restored["projects"][project_root]["mcpServers"]["docs"]["command"],
        "external-new"
    );
}

fn run_mcp_entry_repair(root: &Path) {
    let api = start_api(root);
    let config = root.join("home/.codex/config.toml");
    write(&config, "# host\nmodel = \"host-model\"\n");
    let host_config = read(&config);

    let mcp = api.ok(
        "save_extension",
        json!({ "draft": { "name": "repair-docs", "payload": {
            "kind": "mcp", "transport": "stdio", "command": "npx", "args": ["-y", "repair-docs"]
        } } }),
    );
    let other = api.ok(
        "save_extension",
        json!({ "draft": { "name": "repair-events", "payload": {
            "kind": "mcp", "transport": "http", "url": "https://mcp.example.test/events"
        } } }),
    );
    // Two batches: the workspace contract allows one document write per
    // batch, which is exactly why a repair batch must merge its patches.
    for definition in [&mcp, &other] {
        let view = api.ok(
            "prepare_extension_plan",
            json!({ "request": { "operations": [
                { "operation": "install", "definitionId": definition["id"],
                  "targets": [ { "scope": "app", "client": "codex" } ] }
            ] } }),
        );
        api.ok(
            "apply_extension_plan",
            json!({ "planId": view["planId"], "confirmWrite": true }),
        );
    }
    let deployed = read(&config);
    assert!(deployed.contains("repair-docs"));
    assert!(deployed.contains("repair-events"));

    // The user deletes both managed entries; the rest of the document survives.
    fs::copy(&config, root.join("deployed-config.toml")).unwrap();
    write(&config, &host_config);

    let discovery = api.ok("discover_extensions", json!({}));
    let diagnostics: Vec<Value> = discovery["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|diagnostic| diagnostic["code"] == "managedEntryMissing")
        .cloned()
        .collect();
    assert_eq!(
        diagnostics.len(),
        2,
        "both missing entries are diagnosed as distinct objects"
    );
    let ids: Vec<&str> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic["id"].as_str().unwrap())
        .collect();

    // One repair batch restores both entries as one document patch.
    let repair = api.ok(
        "prepare_extension_repair",
        json!({ "request": { "scanId": discovery["scanId"], "diagnosticIds": ids } }),
    );
    assert_eq!(repair["operations"].as_array().unwrap().len(), 2);
    let outcome = api.ok(
        "apply_extension_plan",
        json!({ "planId": repair["planId"], "confirmWrite": true }),
    );
    assert!(outcome["record"].is_object());

    let restored = read(&config);
    assert!(
        restored.contains("repair-docs"),
        "the first entry came back"
    );
    assert!(
        restored.contains("repair-events"),
        "the second entry came back"
    );
    assert!(
        restored.contains("host-model"),
        "unrelated content survived"
    );
    // Entry order inside the collection may differ from the pre-deletion
    // file; the configuration semantics do not depend on it.
    // A fresh scan shows both problems gone.
    let discovery = api.ok("discover_extensions", json!({}));
    assert!(diagnostic_of(&discovery, "managedEntryMissing").is_none());
    assert!(
        diagnostic_of(&discovery, "managedTargetExternalChange").is_none(),
        "all bindings that share the repaired document receive its final hash"
    );
}
