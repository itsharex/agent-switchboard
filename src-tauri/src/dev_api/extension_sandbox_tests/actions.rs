use super::*;

/// The discovered rows carry backend-judged actions: copy vs manage, the
/// already-in-library state, and the reasons that disable an action before
/// the user ever clicks it.
#[test]
fn discovered_rows_expose_backend_judged_actions() {
    isolated(
        "dev_api::extension_sandbox_tests::actions::discovered_rows_expose_backend_judged_actions",
        run_discovered_actions,
    );
}

fn observation_named<'a>(discovery: &'a Value, name: &str) -> &'a Value {
    discovery["observations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["name"] == name)
        .unwrap_or_else(|| panic!("discovery lacks {name}"))
}

fn run_discovered_actions(root: &Path) {
    // Native state before the app starts: one healthy skill, one directory
    // without a manifest, one skill in the read-only legacy root, and one
    // native MCP entry.
    let skill_dir = root.join("home/.agents/skills/action-helper");
    write(
        &skill_dir.join("SKILL.md"),
        "---\nname: action-helper\ndescription: 动作资格验证\n---\nbody",
    );
    write(
        &root.join("home/.agents/skills/broken-skill/README.md"),
        "not a manifest",
    );
    write(
        &root.join("home/.codex/skills/legacy-skill/SKILL.md"),
        "---\nname: legacy-skill\ndescription: 历史目录\n---\nbody",
    );
    write(
        &root.join("home/.codex/config.toml"),
        "[mcp_servers.native-docs]\ncommand = \"npx\"\nargs = [\"-y\", \"native-docs\"]\n",
    );

    let api = start_api(root);
    let discovery = api.ok("discover_extensions", json!({}));

    // Healthy skill: copy and manage are both offered, nothing in the library.
    let healthy = observation_named(&discovery, "action-helper");
    assert_eq!(healthy["actions"]["import"]["supported"], true);
    assert_eq!(healthy["actions"]["import"]["inLibrary"], false);
    assert_eq!(healthy["actions"]["takeover"]["supported"], true);
    assert_eq!(healthy["managed"], false);

    // Manifest-less directory: both actions are disabled with a reason.
    let broken = observation_named(&discovery, "broken-skill");
    assert_eq!(broken["actions"]["import"]["supported"], false);
    assert!(broken["actions"]["import"]["reason"].is_string());
    assert_eq!(broken["actions"]["takeover"]["supported"], false);

    // Legacy root: copy stays available, management is refused as read-only.
    let legacy = observation_named(&discovery, "legacy-skill");
    assert_eq!(legacy["actions"]["import"]["supported"], true);
    assert_eq!(legacy["actions"]["takeover"]["supported"], false);

    // Copy the healthy skill: the same row now reads as already in the
    // library (content-based, not name-based) and stays unmanaged.
    let copied = api.ok(
        "import_discovered_skill",
        json!({ "observationId": healthy["observationId"] }),
    );
    let discovery = api.ok("discover_extensions", json!({}));
    let healthy = observation_named(&discovery, "action-helper");
    assert_eq!(healthy["actions"]["import"]["inLibrary"], true);
    assert_eq!(healthy["managed"], false);

    // Manage the existing installation: the row becomes managed and names
    // the definition that manages it.
    api.ok(
        "takeover_discovered_extension",
        json!({ "observationId": healthy["observationId"], "confirmWrite": true }),
    );
    let discovery = api.ok("discover_extensions", json!({}));
    let healthy = observation_named(&discovery, "action-helper");
    assert_eq!(healthy["managed"], true);
    assert_eq!(
        healthy["actions"]["managedDefinitionId"], copied["id"],
        "the managed row names its library definition"
    );
}

#[test]
fn project_observations_return_only_the_registered_project_id() {
    isolated(
        "dev_api::extension_sandbox_tests::actions::project_observations_return_only_the_registered_project_id",
        run_project_observation_identity,
    );
}

fn run_project_observation_identity(root: &Path) {
    let api = start_api(root);
    let project_dir = root.join("project-identity");
    fs::create_dir_all(&project_dir).unwrap();
    let project = api.ok(
        "register_project",
        json!({ "root": project_dir.to_string_lossy() }),
    );
    write(
        &project_dir.join(".agents/skills/project-helper/SKILL.md"),
        "---\nname: project-helper\ndescription: project\n---\n",
    );
    let discovery = api.ok("discover_extensions", json!({}));
    let observation = observation_named(&discovery, "project-helper");
    assert_eq!(observation["origin"]["origin"], "projectRoot");
    assert_eq!(observation["origin"]["projectId"], project["id"]);
    assert!(observation["origin"].get("projectPath").is_none());
}
