use super::*;

pub(super) fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

pub(super) fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

pub(super) fn draft_from_profile(profile: &Value) -> Value {
    let mut draft = profile.clone();
    draft
        .as_object_mut()
        .expect("provider profile object")
        .remove("id");
    draft
}

pub(super) fn check_codex(
    config: &Path,
    auth: &Path,
    name: &str,
    secret: &str,
    oauth: &str,
    reasoning_effort: &str,
) {
    let text = read(config);
    assert!(text.contains("model_provider = \"openai\""));
    assert!(!text.contains("[model_providers."));
    assert!(text.contains("openai_base_url = \"http://127.0.0.1:"));
    assert!(text.contains("/codex/"));
    assert!(!text.contains("example.com"));
    assert!(text.contains(&format!("model = \"codex-{name}\"")));
    assert!(
        text.contains(&format!("model_reasoning_effort = \"{reasoning_effort}\"")),
        "Codex config for {name} did not retain the projected reasoning setting; rendered keys: {:?}; reasoning line: {:?}",
        text.lines()
            .filter_map(|line| line.split_once('=').map(|(key, _)| key.trim()))
            .collect::<Vec<_>>(),
        text.lines()
            .find(|line| line.trim_start().starts_with("model_reasoning_effort"))
    );
    assert!(text.contains("web_search = \"live\""));
    assert!(text.contains("[mcp_servers.audit]\ncommand = \"host-only\" # retain bytes"));
    assert!(!text.contains(secret));
    let value: Value = serde_json::from_str(&read(auth)).unwrap();
    assert_eq!(value["auth_mode"], "chatgpt");
    assert!(value["OPENAI_API_KEY"].is_null());
    assert!(value["tokens"]["access_token"] == oauth);
}

pub(super) fn check_claude(path: &Path, name: &str, secret: &str) {
    let value: Value = serde_json::from_str(&read(path)).unwrap();
    assert_eq!(value["model"], format!("claude-{name}"));
    assert_eq!(value["effortLevel"], "high");
    assert_eq!(
        value["env"]["ANTHROPIC_BASE_URL"],
        format!("https://{name}.example.com/v1")
    );
    assert!(value["env"]["ANTHROPIC_API_KEY"] == secret);
    assert!(value["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
    assert_eq!(value["env"]["HOST_SETTING"], "keep");
    assert_eq!(value["permissions"]["deny"], json!(["Read(.env)"]));
}

pub(super) fn check_routed_codex(config: &Path, auth: &Path, upstream_secret: &str) {
    let text = read(config);
    assert!(text.contains("model_provider = \"openai\""));
    assert!(text.contains("openai_base_url = \"http://127.0.0.1:"));
    assert!(text.contains("/codex/"));
    assert!(text.contains("/v1\""));
    assert!(text.contains("web_search = \"disabled\""));
    assert!(!text.contains(upstream_secret));
    let value: Value = serde_json::from_str(&read(auth)).unwrap();
    assert_eq!(value["auth_mode"], "chatgpt");
    assert!(value["OPENAI_API_KEY"].is_null());
    assert!(!read(auth).contains(upstream_secret));
}

pub(super) fn check_routed_claude(path: &Path, upstream_secret: &str) {
    let value: Value = serde_json::from_str(&read(path)).unwrap();
    let endpoint = value["env"]["ANTHROPIC_BASE_URL"].as_str().unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    assert!(!endpoint.contains("example.com"));
    let token = value["env"]["ANTHROPIC_AUTH_TOKEN"].as_str().unwrap();
    assert!(token.starts_with("asb_local_"));
    assert_ne!(token, upstream_secret);
    assert!(value["env"].get("ANTHROPIC_API_KEY").is_none());
}

pub(super) fn check_active_profile(api: &Api, client: &str, profile: &Value) {
    let status = api.ok("config_status", json!({}));
    let active = status
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["app"] == client)
        .unwrap();
    assert_eq!(active["activeProfileId"], profile["id"]);
    assert_eq!(active["syntaxOk"], true);
}
