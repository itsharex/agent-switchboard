use super::*;

#[test]
fn local_skill_authoring_versions_and_rollback() {
    isolated(
        "dev_api::extension_sandbox_tests::authoring::local_skill_authoring_versions_and_rollback",
        run_authoring_workflow,
    );
}

fn run_authoring_workflow(root: &Path) {
    let api = start_api(root);

    // Creating from the template publishes one immutable version.
    let created = api.ok(
        "create_local_skill",
        json!({ "draft": { "name": "note-helper", "description": "整理笔记" } }),
    );
    assert_eq!(created["name"], "note-helper");
    let skill_id = created["id"].as_str().unwrap().to_string();
    let editor = api.ok("get_skill_editor", json!({ "definitionId": skill_id }));
    assert_eq!(editor["editable"], true);
    assert_eq!(editor["revision"], 1);
    assert_eq!(editor["manifest"]["name"], "note-helper");
    let skill_md = editor_file(&editor, "SKILL.md")["text"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(skill_md.contains("name: note-helper"));

    // Invalid names and missing descriptions are rejected before any write.
    api.rejected(
        "create_local_skill",
        json!({ "draft": { "name": "Bad Name", "description": "x" } }),
        "extension-invalid",
    );
    api.rejected(
        "create_local_skill",
        json!({ "draft": { "name": "ok-name", "description": "  " } }),
        "extension-invalid",
    );

    // Editing publishes a second immutable version and keeps the first.
    let edited_md = format!(
        "---\nname: note-helper\ndescription: 整理笔记并归档\n---\n\n# Note Helper\n\n正文步骤。\n"
    );
    let saved = api.ok(
        "update_skill_files",
        json!({
            "definitionId": skill_id,
            "update": {
                "expectedRevision": editor["revision"],
                "expectedDigest": editor["contentDigest"],
                "files": [
                    { "relativePath": "SKILL.md", "text": edited_md },
                    { "relativePath": "steps/collect.md", "text": "# 收集\n\n先收集再整理。\n" }
                ]
            }
        }),
    );
    assert_eq!(saved["revision"], 2);
    let editor2 = api.ok("get_skill_editor", json!({ "definitionId": skill_id }));
    assert_eq!(editor2["revision"], 2);
    let new_digest = editor2["contentDigest"].as_str().unwrap().to_string();
    assert_ne!(new_digest, editor["contentDigest"].as_str().unwrap());
    assert_eq!(
        editor_file(&editor2, "steps/collect.md")["text"]
            .as_str()
            .unwrap(),
        "# 收集\n\n先收集再整理。\n"
    );

    let versions = api
        .ok("list_skill_versions", json!({ "definitionId": skill_id }))
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(versions.len(), 2, "both versions remain stored");
    assert!(versions.iter().any(
        |version| version["digest"] == editor["contentDigest"] && version["isCurrent"] == false
    ));
    assert!(versions
        .iter()
        .any(|version| version["digest"] == new_digest && version["isCurrent"] == true));

    // Stale editors are rejected on both revision and content digest.
    api.rejected(
        "update_skill_files",
        json!({
            "definitionId": skill_id,
            "update": {
                "expectedRevision": editor["revision"],
                "expectedDigest": editor2["contentDigest"],
                "files": [{ "relativePath": "SKILL.md", "text": edited_md }]
            }
        }),
        "extension-conflict",
    );
    api.rejected(
        "update_skill_files",
        json!({
            "definitionId": skill_id,
            "update": {
                "expectedRevision": editor2["revision"],
                "expectedDigest": editor["contentDigest"],
                "files": [{ "relativePath": "SKILL.md", "text": edited_md }]
            }
        }),
        "extension-conflict",
    );

    // Path traversal and duplicate paths never reach the library.
    api.rejected(
        "update_skill_files",
        json!({
            "definitionId": skill_id,
            "update": {
                "expectedRevision": editor2["revision"],
                "expectedDigest": new_digest,
                "files": [{ "relativePath": "../evil.md", "text": "x" }]
            }
        }),
        "extension-invalid",
    );
    api.rejected(
        "update_skill_files",
        json!({
            "definitionId": skill_id,
            "update": {
                "expectedRevision": editor2["revision"],
                "expectedDigest": new_digest,
                "files": [
                    { "relativePath": "a.md", "text": "x" },
                    { "relativePath": "a.md", "text": "y" }
                ]
            }
        }),
        "extension-invalid",
    );

    // A broken manifest is rejected without a new version.
    api.rejected(
        "update_skill_files",
        json!({
            "definitionId": skill_id,
            "update": {
                "expectedRevision": editor2["revision"],
                "expectedDigest": new_digest,
                "files": [{ "relativePath": "SKILL.md", "text": "# no frontmatter\n" }]
            }
        }),
        "extension-invalid",
    );
    let versions_after_rejects = api
        .ok("list_skill_versions", json!({ "definitionId": skill_id }))
        .as_array()
        .unwrap()
        .len();
    assert_eq!(versions_after_rejects, 2, "rejected edits store nothing");

    // An unchanged save writes nothing.
    let unchanged = api.ok(
        "update_skill_files",
        json!({
            "definitionId": skill_id,
            "update": {
                "expectedRevision": editor2["revision"],
                "expectedDigest": new_digest,
                "files": [
                    { "relativePath": "SKILL.md", "text": edited_md },
                    { "relativePath": "steps/collect.md", "text": "# 收集\n\n先收集再整理。\n" }
                ]
            }
        }),
    );
    assert_eq!(
        unchanged["revision"], 2,
        "unchanged save keeps the revision"
    );

    // Rolling back through update_skill_definition restores the first text
    // without needing a source scan.
    let rolled = api.ok(
        "update_skill_definition",
        json!({
            "definitionId": skill_id,
            "newDigest": editor["contentDigest"]
        }),
    );
    assert_eq!(rolled["revision"], 3);
    let editor3 = api.ok("get_skill_editor", json!({ "definitionId": skill_id }));
    assert_eq!(
        editor3["contentDigest"], editor["contentDigest"],
        "rollback points at the stored first version"
    );
    assert_eq!(
        editor_file(&editor3, "SKILL.md")["text"].as_str().unwrap(),
        skill_md
    );
    assert!(
        !editor3["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file["relativePath"] == "steps/collect.md"),
        "the first version has no extra file"
    );
}

// ------------------------------------------------------------- fork workflow

#[test]
fn fork_copies_content_into_a_portable_local_definition() {
    isolated(
        "dev_api::extension_sandbox_tests::authoring::fork_copies_content_into_a_portable_local_definition",
        run_fork_workflow,
    );
}

fn run_fork_workflow(root: &Path) {
    let api = start_api(root);

    // A host-scoped source with a binary asset imported from disk.
    let source_root = root.join("skills-repo");
    write(
        &source_root.join("helper/SKILL.md"),
        "---\nname: repo-helper\nallowed-tools:\n  - Read\n---\n\n# Repo Helper\n",
    );
    let logo: Vec<u8> = vec![
        0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0xfe, 0x00,
    ];
    write_bytes(&source_root.join("helper/assets/logo.png"), &logo);
    let candidates = api
        .ok(
            "scan_local_skill_source",
            json!({ "root": source_root.to_string_lossy() }),
        )
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(candidates.len(), 1, "one skill in the source");
    let digest = candidates[0]["digest"].as_str().unwrap().to_string();
    let imported = api.ok(
        "import_skill_candidate",
        json!({ "digest": digest, "name": "repo-helper", "hostScoped": "claude" }),
    );
    let source_id = imported["id"].as_str().unwrap().to_string();

    // Host-scoped skills are not directly editable.
    let view = api.ok("get_skill_editor", json!({ "definitionId": source_id }));
    assert_eq!(view["editable"], false);
    api.rejected(
        "update_skill_files",
        json!({
            "definitionId": source_id,
            "update": {
                "expectedRevision": view["revision"],
                "expectedDigest": view["contentDigest"],
                "files": [{ "relativePath": "SKILL.md", "text": "x" }]
            }
        }),
        "extension-invalid",
    );

    // The source lacks description, so the portable copy needs an edit that
    // adds one before validate_definition accepts it.
    let rejection = api.rejected(
        "fork_local_skill",
        json!({ "definitionId": source_id }),
        "extension-invalid",
    );
    assert!(
        rejection["message"]
            .as_str()
            .unwrap()
            .contains("description"),
        "rejection explains the missing portable field"
    );

    // Re-import with a description in the content; the fork then succeeds.
    write(
        &source_root.join("helper/SKILL.md"),
        "---\nname: repo-helper\ndescription: 仓库中的助手\nallowed-tools:\n  - Read\n---\n\n# Repo Helper\n",
    );
    let candidates = api
        .ok(
            "scan_local_skill_source",
            json!({ "root": source_root.to_string_lossy() }),
        )
        .as_array()
        .unwrap()
        .clone();
    let digest2 = candidates[0]["digest"].as_str().unwrap().to_string();
    let imported2 = api.ok(
        "import_skill_candidate",
        json!({ "digest": digest2, "name": "repo-helper", "hostScoped": "claude" }),
    );
    let source_id2 = imported2["id"].as_str().unwrap().to_string();
    let forked = api.ok("fork_local_skill", json!({ "definitionId": source_id2 }));
    let forked_id = forked["id"].as_str().unwrap().to_string();
    assert_ne!(forked_id, source_id2);

    let editor = api.ok("get_skill_editor", json!({ "definitionId": forked_id }));
    assert_eq!(editor["editable"], true);
    assert_eq!(
        editor["manifest"]["description"], "仓库中的助手",
        "the copy carries the manifest"
    );
    // Binary assets are listed without their bytes.
    let logo_entry = editor_file(&editor, "assets/logo.png");
    assert!(logo_entry["text"].is_null(), "binary files stay opaque");
    assert_eq!(logo_entry["size"], logo.len() as u64);

    // Editing text keeps the binary bytes verbatim in the new version.
    let edited_md =
        "---\nname: repo-helper\ndescription: 本地改写\n---\n\n# 本地副本\n".to_string();
    api.ok(
        "update_skill_files",
        json!({
            "definitionId": forked_id,
            "update": {
                "expectedRevision": editor["revision"],
                "expectedDigest": editor["contentDigest"],
                "files": [{ "relativePath": "SKILL.md", "text": edited_md }]
            }
        }),
    );
    let new_editor = api.ok("get_skill_editor", json!({ "definitionId": forked_id }));
    let new_digest = new_editor["contentDigest"].as_str().unwrap();
    let stored_logo = fs::read(root.join(format!(
        "app-data/state/extensions/library/{forked_id}/{new_digest}/assets/logo.png"
    )))
    .unwrap();
    assert_eq!(stored_logo, logo, "binary content survives the edit");

    // Forking an already-local skill is refused.
    api.rejected(
        "fork_local_skill",
        json!({ "definitionId": forked_id }),
        "extension-invalid",
    );
    // Forking an MCP definition is refused.
    let mcp = api.ok(
        "save_extension",
        json!({ "draft": { "name": "docs", "payload": {
            "kind": "mcp", "transport": "stdio", "command": "npx", "args": ["-y", "docs"]
        } } }),
    );
    api.rejected(
        "fork_local_skill",
        json!({ "definitionId": mcp["id"] }),
        "extension-invalid",
    );

    // Dependency links reference real MCP definitions only.
    api.ok(
        "update_skill_dependencies",
        json!({
            "definitionId": forked_id,
            "update": {
                "expectedRevision": 2,
                "dependencies": [ { "name": "docs", "resourceId": mcp["id"] } ]
            }
        }),
    );
    api.rejected(
        "update_skill_dependencies",
        json!({
            "definitionId": forked_id,
            "update": {
                "expectedRevision": 3,
                "dependencies": [ { "name": "not-mcp", "resourceId": forked_id } ]
            }
        }),
        "extension-invalid",
    );
    api.rejected(
        "update_skill_dependencies",
        json!({
            "definitionId": forked_id,
            "update": {
                "expectedRevision": 3,
                "dependencies": [ { "name": "ghost", "resourceId": "ext-missing" } ]
            }
        }),
        "extension-invalid",
    );
    let workspace = api.ok("list_extensions", json!({}));
    let item = workspace["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["id"] == forked_id)
        .unwrap()
        .clone();
    assert_eq!(
        item["dependencies"][0]["resourceId"], mcp["id"],
        "the list projection carries the dependency link"
    );
    assert_eq!(
        item["dependencyStates"][0]["state"], "targetUnsupported",
        "a linked MCP without a matching binding on this skill's targets derives unsupported"
    );
}

// ---------------------------------------------------------- combined batches
