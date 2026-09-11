//! Browser-command coverage for the current, dedicated Codex profile API.

use super::{serve, Server};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;
use tauri::Manager;

const CHILD_ROOT: &str = "ASB_CODEX_SWITCH_TEST_ROOT";
const ORIGIN: &str = "http://127.0.0.1:1420";
const TEST_NAME: &str =
    "dev_api::sandbox_tests::client_settings_then_new_providers_switch_through_real_commands";

#[test]
fn client_settings_then_new_providers_switch_through_real_commands() {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        run_workflow(Path::new(&root));
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let home = root.join("home");
    fs::create_dir_all(home.join("AppData/Roaming")).unwrap();
    fs::create_dir_all(home.join("AppData/Local")).unwrap();
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(CHILD_ROOT, root)
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .env("APPDATA", home.join("AppData/Roaming"))
        .env("LOCALAPPDATA", home.join("AppData/Local"))
        .env("CODEX_HOME", root.join("codex"))
        .env("CLAUDE_CONFIG_DIR", root.join("claude"))
        .status()
        .unwrap();
    assert!(status.success(), "isolated Codex command workflow failed");
}

struct Api {
    url: String,
    client: reqwest::blocking::Client,
}

impl Api {
    fn ok(&self, command: &str, args: Value) -> Value {
        let response = self
            .client
            .post(&self.url)
            .header("Origin", ORIGIN)
            .header("Content-Type", "application/json")
            .body(json!({ "command": command, "args": args }).to_string())
            .send()
            .unwrap();
        assert_eq!(response.status(), 200);
        let response: Value = serde_json::from_str(&response.text().unwrap()).unwrap();
        assert_eq!(
            response["kind"], "success",
            "{command}: {}",
            response["error"]
        );
        response["result"].clone()
    }
}

fn run_workflow(root: &Path) {
    let codex = root.join("codex");
    let config = codex.join("config.toml");
    let auth = codex.join("auth.json");
    fs::create_dir_all(&codex).unwrap();
    fs::write(
        &config,
        "model_provider = \"openai\"\nmodel = \"host-model\"\n",
    )
    .unwrap();
    let auth_bytes = br#"{"auth_mode":"chatgpt","tokens":{"access_token":"fixture-access","refresh_token":"fixture-refresh","id_token":"fixture-id"}}"#;
    fs::write(&auth, auth_bytes).unwrap();

    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    context.config_mut().identifier = root.join("app-data").to_string_lossy().into_owned();
    let app = tauri::Builder::default()
        .any_thread()
        .build(context)
        .unwrap();
    let state = crate::local_state::LocalState::from_app(app.handle()).unwrap();
    state.initialize_schemas().unwrap();
    app.manage(crate::gateway::GatewayController::start(&state));
    app.manage(crate::gateway::PortChangePreparations::default());
    app.manage(crate::commands::ConfigWriteGate::default());
    app.manage(crate::commands::switching::ProfileSavePreparations::default());
    app.manage(crate::commands::switching::CodexProfileSavePreparations::default());
    let server = Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/invoke", server.server_addr());
    let handle = app.handle().clone();
    thread::spawn(move || serve(server, handle, ORIGIN.to_string()));
    let api = Api {
        url,
        client: reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap(),
    };

    let first = create_profile(&api, "first", "first-secret", "https://first.example/v1");
    let first_preview = api.ok("preview_switch", json!({ "profileId": first["id"] }));
    api.ok(
        "execute_switch",
        json!({
            "profileId": first["id"],
            "expectedHash": first_preview["contentHash"],
            "expectedRenderedHash": first_preview["renderedHash"],
            "confirmWrite": true,
        }),
    );
    let first_config = fs::read_to_string(&config).unwrap();
    assert!(first_config.contains("model_provider = \"openai\""));
    assert!(first_config.contains("/codex/asb_codex_"));
    assert!(!first_config.contains("first-secret"));
    assert_eq!(fs::read(&auth).unwrap(), auth_bytes);

    let second = create_profile(&api, "second", "second-secret", "https://second.example/v1");
    let second_preview = api.ok("preview_switch", json!({ "profileId": second["id"] }));
    api.ok(
        "execute_switch",
        json!({
            "profileId": second["id"],
            "expectedHash": second_preview["contentHash"],
            "expectedRenderedHash": second_preview["renderedHash"],
            "confirmWrite": true,
        }),
    );
    let second_config = fs::read_to_string(&config).unwrap();
    assert_eq!(
        gateway_base_url(&first_config),
        gateway_base_url(&second_config)
    );
    assert!(!second_config.contains("second-secret"));
    assert_eq!(fs::read(&auth).unwrap(), auth_bytes);

    // Official Codex is one more stored profile: create it in the generic
    // store, then switch to it through the same preview-and-confirm command
    // every other profile uses.
    let official_parameters = api.ok(
        "get_provider_parameters_catalog",
        json!({ "target": "codex" }),
    )["defaults"]
        .clone();
    let preparation = api.ok(
        "prepare_profile_save",
        json!({
            "profileId": null,
            "expectedFileHash": null,
            "draft": {
                "app": "codex",
                "routeMode": "official",
                "name": "Codex 官方登录",
                "baseUrl": null,
                "apiKey": "",
                "upstreamProtocol": null,
                "maxOutputTokens": null,
                "model": null,
                "parameters": official_parameters,
                "websiteUrl": null
            }
        }),
    );
    let official = api.ok(
        "commit_profile_save",
        json!({
            "preparationId": preparation["preparationId"],
            "confirmWrite": true,
        }),
    );
    let official_preview = api.ok(
        "preview_switch",
        json!({ "profileId": official["profile"]["id"] }),
    );
    api.ok(
        "execute_switch",
        json!({
            "profileId": official["profile"]["id"],
            "expectedHash": official_preview["contentHash"],
            "expectedRenderedHash": official_preview["renderedHash"],
            "confirmWrite": true,
        }),
    );
    assert!(!fs::read_to_string(&config)
        .unwrap()
        .contains("openai_base_url"));
    assert_eq!(fs::read(&auth).unwrap(), auth_bytes);
}

fn create_profile(api: &Api, name: &str, api_key: &str, endpoint: &str) -> Value {
    let parameters = api.ok(
        "get_provider_parameters_catalog",
        json!({ "target": "codex" }),
    )["defaults"]
        .clone();
    api.ok(
        "create_codex_profile",
        json!({
            "draft": {
                "name": name,
                "endpoint": endpoint,
                "apiKey": api_key,
                "upstream": "responses",
                "requestMode": "standard",
                "defaultModel": "codex-test",
                "catalog": [{
                    "id": "codex-test", "contextWindow": 128000, "maxOutputTokens": 16384,
                    "functionTools": true, "customTools": true, "toolSearch": true,
                    "reasoning": true, "defaultReasoningLevel": "high",
                    "supportedReasoningLevels": ["none", "high"], "images": false, "compact": true
                }],
                "modelRoutes": [],
                "capabilities": {
                    "responses": true, "compact": true, "models": true, "chatCompletions": true,
                    "alphaSearch": false, "imageGeneration": false, "imageEdit": false,
                    "functionTools": true, "customTools": true, "toolSearch": true, "reasoning": true,
                    "chatReasoning": { "kind": "unsupported" }
                },
                "parameters": parameters,
                "notes": null, "websiteUrl": null, "usageQuery": null
            }
        }),
    )["profile"]
        .clone()
}

fn gateway_base_url(config: &str) -> &str {
    config
        .lines()
        .find_map(|line| {
            line.strip_prefix("openai_base_url = \"")
                .and_then(|value| value.strip_suffix("\""))
        })
        .unwrap()
}
