//! Black-box HTTP tests against the production command dispatcher and filesystem.
//! Each child process owns a temporary home, client directories and Tauri app data.
//! Store, adapter and executor are real; credentials are generated dummy values.
//! No external provider is contacted.

use super::*;
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use tauri::Manager;

const CHILD_ROOT: &str = "ASB_SWITCH_TEST_ROOT";
const ORIGIN: &str = "http://127.0.0.1:1420";

#[test]
fn common_settings_then_new_providers_switch_through_real_commands() {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        run_workflow(Path::new(&root));
        return;
    }
    for redirected in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let home = root.join("home");
        fs::create_dir_all(home.join("AppData/Roaming")).unwrap();
        fs::create_dir_all(home.join("AppData/Local")).unwrap();
        let mut command = Command::new(std::env::current_exe().unwrap());
        command.args([
            "--exact",
            "dev_api::sandbox_tests::common_settings_then_new_providers_switch_through_real_commands",
            "--nocapture",
        ]);
        command
            .env(CHILD_ROOT, root)
            .env("USERPROFILE", &home)
            .env("HOME", &home)
            .env("APPDATA", home.join("AppData/Roaming"))
            .env("LOCALAPPDATA", home.join("AppData/Local"))
            .env_remove("CODEX_HOME")
            .env_remove("CLAUDE_CONFIG_DIR");
        if redirected {
            command
                .env("CODEX_HOME", root.join("codex-override"))
                .env("CLAUDE_CONFIG_DIR", root.join("claude-override"));
        }
        let mut child = command.spawn().unwrap();
        let started = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "sandbox child failed (redirected={redirected})"
                );
                break;
            }
            if started.elapsed() > Duration::from_secs(60) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("sandbox command workflow timed out");
            }
            thread::sleep(Duration::from_millis(25));
        }
    }
}

struct Api {
    url: String,
    client: reqwest::blocking::Client,
}

impl Api {
    fn request(&self, command: &str, args: Value) -> Value {
        let response = self
            .client
            .post(&self.url)
            .header("Origin", ORIGIN)
            .header("Content-Type", "application/json")
            .body(json!({ "command": command, "args": args }).to_string())
            .send()
            .expect("sandbox HTTP request");
        assert_eq!(response.status(), 200);
        serde_json::from_str(&response.text().unwrap()).unwrap()
    }

    fn ok(&self, command: &str, args: Value) -> Value {
        let response = self.request(command, args);
        assert_eq!(
            response["kind"], "success",
            "{command}: {}",
            response["error"]
        );
        response["result"].clone()
    }

    fn rejected(&self, command: &str, args: Value, code: &str) {
        let response = self.request(command, args);
        assert_eq!(response["kind"], "failure", "{command} must reject");
        assert_eq!(response["error"]["code"], code);
    }

    fn save_common(&self, app: &str, key: &str, value: Value) -> Value {
        let editor = self.ok("get_common_settings_editor", json!({ "target": app }));
        let mut settings = editor["settings"].clone();
        settings["settings"][key] = json!({ "mode": "explicit", "value": value });
        self.ok(
            "save_common_settings",
            json!({
                "target": app, "settings": settings, "expectedSettingsHash": editor["settingsHash"]
            }),
        )
    }

    fn create(
        &self,
        app: &str,
        name: &str,
        official: bool,
        secret: &str,
        upstream_protocol: &str,
    ) -> Value {
        let prepared = self.ok("prepare_profile_save", json!({
            "profileId": null,
            "expectedFileHash": null,
            "draft": {
            "app": app, "routeMode": if official { "official" } else { "custom" },
            "name": name, "apiKey": if official { "" } else { secret },
            "baseUrl": if official { Value::Null } else { json!(format!("https://{name}.example.com/v1")) },
            "upstreamProtocol": if official { Value::Null } else { json!(upstream_protocol) },
            "maxOutputTokens": if app == "codex" && upstream_protocol == "anthropicMessages" { json!(8192) } else { Value::Null },
            "model": if official { Value::Null } else { json!(format!("{app}-{name}")) }, "websiteUrl": null
        }}));
        assert_eq!(prepared["kind"], "create");
        self.ok(
            "commit_profile_save",
            json!({"preparationId": prepared["preparationId"], "confirmWrite": false}),
        )["profile"]
            .clone()
    }

    fn preview(&self, profile: &Value) -> Value {
        self.ok("preview_switch", json!({ "profileId": profile["id"] }))
    }

    fn provider_record(&self, profile: &Value) -> Value {
        self.ok("list_profiles", json!({}))
            .as_array()
            .expect("provider records")
            .iter()
            .find(|record| record["profile"]["id"] == profile["id"])
            .expect("created profile record")
            .clone()
    }

    fn prepare_edit(&self, record: &Value, draft: Value) -> Value {
        self.ok(
            "prepare_profile_save",
            json!({
                "profileId": record["profile"]["id"],
                "draft": draft,
                "expectedFileHash": record["fileHash"],
            }),
        )
    }

    fn commit_profile_save(&self, prepared: &Value, confirm_write: bool) -> Value {
        self.ok(
            "commit_profile_save",
            json!({
                "preparationId": prepared["preparationId"],
                "confirmWrite": confirm_write,
            }),
        )
    }

    fn switch_args(profile: &Value, preview: &Value, confirm: bool) -> Value {
        json!({ "profileId": profile["id"], "expectedHash": preview["contentHash"],
            "expectedRenderedHash": preview["renderedHash"], "confirmWrite": confirm })
    }

    fn switch(&self, profile: &Value, secret: &str) -> Value {
        let preview = self.preview(profile);
        assert!(
            !preview.to_string().contains(secret),
            "preview leaked a fixture secret"
        );
        let outcome = self.ok("execute_switch", Self::switch_args(profile, &preview, true));
        assert!(
            !outcome.to_string().contains(secret),
            "outcome leaked a fixture secret"
        );
        outcome
    }
}

fn prepare_gateway_port_change(api: &Api, excluded_port: u16) -> Value {
    for _ in 0..20 {
        let probe = Server::http(("127.0.0.1", 0)).unwrap();
        let port = probe.server_addr().to_ip().unwrap().port();
        drop(probe);
        if port == excluded_port {
            continue;
        }
        let response = api.request("gateway_prepare_port_change", json!({ "newPort": port }));
        if response["kind"].as_str() == Some("success") {
            return response["result"].clone();
        }
        assert_eq!(response["error"]["code"], "gateway-port-change-invalid");
    }
    panic!("could not reserve a gateway test port")
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn draft_from_profile(profile: &Value) -> Value {
    let mut draft = profile.clone();
    draft
        .as_object_mut()
        .expect("provider profile object")
        .remove("id");
    draft
}

fn run_workflow(root: &Path) {
    // Only the subprocess environment is redirected; parent/user variables stay intact.
    let home = root.join("home");
    let codex = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or(home.join(".codex"));
    let claude = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or(home.join(".claude"));
    let config = codex.join("config.toml");
    let auth = codex.join("auth.json");
    let settings = claude.join("settings.json");
    let secret = uuid::Uuid::new_v4().to_string();
    let secret_b = uuid::Uuid::new_v4().to_string();
    let oauth = uuid::Uuid::new_v4().to_string();
    let initial_codex = "# host comment\nmodel = \"host-model\"\n[mcp_servers.audit]\ncommand = \"host-only\" # retain bytes\n";
    let initial_claude =
        "{\"permissions\":{\"deny\":[\"Read(.env)\"]},\"env\":{\"HOST_SETTING\":\"keep\"}}";
    write(&config, initial_codex);
    write(
        &auth,
        &json!({"auth_mode":"chatgpt", "tokens":{"access_token":oauth}, "host":"keep"}).to_string(),
    );
    write(&settings, initial_claude);
    let initial_auth = read(&auth);
    // Sentinel default files must stay untouched when explicit directories are used.
    if codex != home.join(".codex") {
        write(&home.join(".codex/config.toml"), "default sentinel");
    }
    if claude != home.join(".claude") {
        write(&home.join(".claude/settings.json"), "default sentinel");
    }

    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    // Tauri joins this absolute identifier to app_data_dir: no real app state is used.
    context.config_mut().identifier = root.join("app-data").to_string_lossy().into_owned();
    let app = tauri::Builder::default()
        .any_thread()
        .build(context)
        .unwrap();
    assert_eq!(app.path().app_data_dir().unwrap(), root.join("app-data"));
    let state = crate::local_state::LocalState::from_app(app.handle()).unwrap();
    app.manage(crate::gateway::GatewayController::start(&state));
    app.manage(crate::gateway::PortChangePreparations::default());
    app.manage(crate::commands::ConfigWriteGate::default());
    app.manage(crate::commands::switching::ProfileSavePreparations::default());
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

    // The browser-development dispatcher exposes the same one-shot
    // prepare/confirm/cancel contract as the desktop invoke boundary.
    let initial_gateway = api.ok("gateway_status", json!({}));
    let prepared_port = prepare_gateway_port_change(
        &api,
        initial_gateway["configuredPort"].as_u64().unwrap() as u16,
    );
    api.rejected(
        "gateway_commit_port_change",
        json!({ "preparationId": prepared_port["preparationId"], "confirmWrite": false }),
        "write-not-confirmed",
    );
    let committed_port = api.ok(
        "gateway_commit_port_change",
        json!({ "preparationId": prepared_port["preparationId"], "confirmWrite": true }),
    );
    assert_eq!(committed_port["toPort"], prepared_port["toPort"]);
    assert_eq!(committed_port["warnings"], json!([]));

    let cancelled_port =
        prepare_gateway_port_change(&api, committed_port["toPort"].as_u64().unwrap() as u16);
    api.ok(
        "gateway_cancel_port_change",
        json!({ "preparationId": cancelled_port["preparationId"] }),
    );
    let released = Server::http((
        "127.0.0.1",
        cancelled_port["toPort"].as_u64().unwrap() as u16,
    ));
    assert!(
        released.is_ok(),
        "cancelled preview must release its socket"
    );
    drop(released);

    // Authentication is derived by `upstreamProtocol`; obsolete manual
    // authentication input must fail at the public command boundary before
    // either command can contact a provider.
    api.rejected(
        "fetch_provider_models",
        json!({
            "request": {
                "url": "https://provider.example/v1",
                "apiKey": &secret,
                "upstreamProtocol": "responses",
                "authScheme": "bearer"
            }
        }),
        "web-argument-invalid",
    );
    api.rejected(
        "test_usage_query",
        json!({
            "request": {
                "query": {
                    "kind": "declarative",
                    "url": "https://provider.example/usage",
                    "remainingPath": "balance",
                    "refreshIntervalMinutes": 0
                },
                "apiKey": &secret,
                "baseUrl": "https://provider.example/v1",
                "upstreamProtocol": "responses",
                "authScheme": "bearer"
            }
        }),
        "web-argument-invalid",
    );

    // First save general settings, then create providers through the real public commands.
    let codex_common = api.save_common("codex", "model_reasoning_effort", json!("high"));
    assert_eq!(
        codex_common["settings"]["settings"]["model_reasoning_effort"]["value"],
        "high"
    );
    let codex_common = api.save_common("codex", "web_search", json!("live"));
    assert_eq!(
        codex_common["settings"]["settings"]["model_reasoning_effort"]["value"],
        "high"
    );
    assert_eq!(
        codex_common["settings"]["settings"]["web_search"]["value"],
        "live"
    );
    api.save_common("claude", "effortLevel", json!("high"));
    let codex_a = api.create("codex", "a", false, &secret, "responses");
    let codex_b = api.create("codex", "b", false, &secret_b, "responses");
    let claude_a = api.create("claude", "a", false, &secret, "anthropicMessages");
    let claude_b = api.create("claude", "b", false, &secret_b, "anthropicMessages");

    // Every save path starts with the same preparation. A new profile and an
    // unchanged edit write neither client config nor a switch backup.
    let codex_a_record = api.provider_record(&codex_a);
    let no_change = api.prepare_edit(&codex_a_record, draft_from_profile(&codex_a));
    assert_eq!(no_change["kind"], "noChange");
    assert!(no_change["preview"].is_null());
    api.commit_profile_save(&no_change, false);

    // Editing an inactive provider's effective field persists only its own
    // profile. Restoring the original draft uses the same path again.
    let codex_b_record = api.provider_record(&codex_b);
    let mut inactive_draft = draft_from_profile(&codex_b);
    inactive_draft["model"] = json!("codex-b-temporary");
    let inactive_save = api.prepare_edit(&codex_b_record, inactive_draft);
    assert_eq!(inactive_save["kind"], "saveOnly");
    assert!(inactive_save["preview"].is_null());
    let inactive_saved = api.commit_profile_save(&inactive_save, false);
    assert_eq!(inactive_saved["profile"]["model"], "codex-b-temporary");
    let inactive_restore = api.prepare_edit(&inactive_saved, draft_from_profile(&codex_b));
    assert_eq!(inactive_restore["kind"], "saveOnly");
    api.commit_profile_save(&inactive_restore, false);
    assert_eq!(read(&config), initial_codex);
    assert_eq!(read(&settings), initial_claude);
    assert!(read(&auth) == initial_auth);
    let before = api.preview(&codex_a);
    assert!(!before.to_string().contains(&oauth));
    assert!(
        before["content"]
            .as_str()
            .is_some_and(|content| content.contains("model_reasoning_effort = \"high\"")),
        "Codex preview must project the saved common reasoning preference"
    );
    assert!(
        before["content"]
            .as_str()
            .is_some_and(|content| content.contains("web_search = \"live\"")),
        "Codex preview must project the saved common web-search preference"
    );
    assert_eq!(read(&config), initial_codex);
    assert!(read(&auth) == initial_auth);
    api.rejected(
        "execute_switch",
        Api::switch_args(&codex_a, &before, false),
        "write-not-confirmed",
    );
    assert_eq!(read(&config), initial_codex);
    let codex_a_outcome = api.switch(&codex_a, &secret);
    assert!(
        codex_a_outcome["preview"]["changes"]
            .as_array()
            .is_some_and(|changes| changes.iter().any(|change| {
                change["key"] == "model_reasoning_effort" && change["after"] == "high"
            })),
        "the committed Codex projection must retain the previewed reasoning value"
    );
    assert_eq!(
        codex_a_outcome["finalHash"],
        asb_switch::sha256_hex(&read(&config)),
        "the Codex file must still equal the executor's committed candidate"
    );
    check_codex(&config, &auth, "a", &secret, &oauth, "high");

    // An active effective edit requires the explicit confirmation and is
    // applied atomically with the provider file. A rejected confirmation
    // leaves both client files untouched and consumes the old preparation.
    let active_record = api.provider_record(&codex_a);
    let mut active_draft = draft_from_profile(&codex_a);
    active_draft["model"] = json!("codex-a-temporary");
    let active_save = api.prepare_edit(&active_record, active_draft.clone());
    assert_eq!(active_save["kind"], "saveAndApply");
    assert!(active_save["preview"].is_object());
    let before_confirmed_save = read(&config);
    let before_confirmed_auth = read(&auth);
    api.rejected(
        "commit_profile_save",
        json!({"preparationId": active_save["preparationId"], "confirmWrite": false}),
        "write-not-confirmed",
    );
    api.rejected(
        "commit_profile_save",
        json!({"preparationId": active_save["preparationId"], "confirmWrite": true}),
        "profile-save-stale",
    );
    assert_eq!(read(&config), before_confirmed_save);
    assert_eq!(read(&auth), before_confirmed_auth);
    let active_save = api.prepare_edit(&active_record, active_draft);
    let active_saved = api.commit_profile_save(&active_save, true);
    assert_eq!(active_saved["profile"]["model"], "codex-a-temporary");
    assert!(read(&config).contains("model = \"codex-a-temporary\""));
    check_active_profile(&api, "codex", &codex_a);
    let active_restore = api.prepare_edit(&active_saved, draft_from_profile(&codex_a));
    assert_eq!(active_restore["kind"], "saveAndApply");
    api.commit_profile_save(&active_restore, true);
    check_codex(&config, &auth, "a", &secret, &oauth, "high");

    // Metadata on an active provider is persisted without re-projecting the
    // client files.
    let metadata_record = api.provider_record(&codex_a);
    let mut metadata_draft = draft_from_profile(&codex_a);
    metadata_draft["notes"] = json!("metadata only");
    let metadata_save = api.prepare_edit(&metadata_record, metadata_draft);
    assert_eq!(metadata_save["kind"], "saveOnly");
    let before_metadata_config = read(&config);
    let before_metadata_auth = read(&auth);
    api.commit_profile_save(&metadata_save, false);
    assert_eq!(read(&config), before_metadata_config);
    assert_eq!(read(&auth), before_metadata_auth);

    // The durable marker is discarded when a process stopped before the
    // provider file changed, and otherwise deterministically finishes the
    // already confirmed provider update on the next startup/write boundary.
    let recovery_record = api.provider_record(&codex_a);
    state
        .configuration()
        .begin_profile_save(&crate::config_store::PendingProfileSave {
            profile_id: codex_a["id"].as_str().unwrap().to_string(),
            app: asb_core::contracts::AppKind::Codex,
            previous_file_hash: recovery_record["fileHash"].as_str().unwrap().to_string(),
        })
        .unwrap();
    api.save_common("codex", "web_search", json!("live"));
    assert!(state
        .configuration()
        .pending_profile_save()
        .unwrap()
        .is_none());
    assert_eq!(read(&config), before_metadata_config);

    let mut interrupted_draft = draft_from_profile(&codex_a);
    interrupted_draft["model"] = json!("codex-a-recovered");
    state
        .configuration()
        .begin_profile_save(&crate::config_store::PendingProfileSave {
            profile_id: codex_a["id"].as_str().unwrap().to_string(),
            app: asb_core::contracts::AppKind::Codex,
            previous_file_hash: recovery_record["fileHash"].as_str().unwrap().to_string(),
        })
        .unwrap();
    state
        .configuration()
        .update_provider(
            codex_a["id"].as_str().unwrap(),
            serde_json::from_value(interrupted_draft).unwrap(),
            recovery_record["fileHash"].as_str().unwrap(),
        )
        .unwrap();
    api.save_common("codex", "web_search", json!("live"));
    assert!(state
        .configuration()
        .pending_profile_save()
        .unwrap()
        .is_none());
    assert!(read(&config).contains("model = \"codex-a-recovered\""));
    let recovered_record = api.provider_record(&codex_a);
    let recovered_restore = api.prepare_edit(&recovered_record, draft_from_profile(&codex_a));
    assert_eq!(recovered_restore["kind"], "saveAndApply");
    api.commit_profile_save(&recovered_restore, true);
    check_codex(&config, &auth, "a", &secret, &oauth, "high");
    assert_eq!(read(&settings), initial_claude);
    api.switch(&claude_a, &secret);
    check_claude(&settings, "a", &secret);

    // Claude follows the same active save-and-apply transaction and restores
    // its original draft before the wider switch workflow continues.
    let claude_record = api.provider_record(&claude_a);
    let mut claude_draft = draft_from_profile(&claude_a);
    claude_draft["model"] = json!("claude-a-temporary");
    let claude_save = api.prepare_edit(&claude_record, claude_draft);
    assert_eq!(claude_save["kind"], "saveAndApply");
    let claude_saved = api.commit_profile_save(&claude_save, true);
    assert_eq!(claude_saved["profile"]["model"], "claude-a-temporary");
    assert_eq!(
        serde_json::from_str::<Value>(&read(&settings)).unwrap()["model"],
        "claude-a-temporary"
    );
    let claude_restore = api.prepare_edit(&claude_saved, draft_from_profile(&claude_a));
    assert_eq!(claude_restore["kind"], "saveAndApply");
    api.commit_profile_save(&claude_restore, true);
    check_claude(&settings, "a", &secret);
    let a_config = read(&config);
    let a_auth = read(&auth);
    let a_settings = read(&settings);
    api.switch(&codex_b, &secret_b);
    api.switch(&claude_b, &secret_b);
    check_codex(&config, &auth, "b", &secret_b, &oauth, "high");
    check_claude(&settings, "b", &secret_b);
    assert!(!read(&auth).contains(&secret));
    assert!(!read(&settings).contains(&secret));

    for (client, profile) in [("codex", &codex_b), ("claude", &claude_b)] {
        let status = api.ok("config_status", json!({}));
        let status = status
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["app"] == client)
            .unwrap();
        assert_eq!(status["activeProfileId"], profile["id"]);
        assert_eq!(status["syntaxOk"], true);
        api.ok(
            "undo_last_switch",
            json!({"target":client,"confirmWrite":true}),
        );
    }
    assert_eq!(read(&config), a_config);
    assert!(read(&auth) == a_auth);
    assert!(read(&settings) == a_settings);

    // A common-setting edit invalidates a previously accepted projection candidate.
    let stale = api.preview(&codex_b);
    api.save_common("codex", "model_reasoning_effort", json!("xhigh"));
    api.rejected(
        "execute_switch",
        Api::switch_args(&codex_b, &stale, true),
        "preview-stale",
    );
    assert_eq!(read(&config), a_config);
    api.switch(&codex_b, &secret_b);
    assert!(read(&config).contains("model_reasoning_effort = \"xhigh\""));
    let stale = api.preview(&claude_b);
    let changed = format!("{}\n", read(&settings));
    write(&settings, &changed);
    api.rejected(
        "execute_switch",
        Api::switch_args(&claude_b, &stale, true),
        "external-change",
    );
    assert!(read(&settings) == changed);

    // Every client can select each non-native upstream protocol. The real
    // command dispatcher must install a loopback-only endpoint and a distinct
    // local capability token; the configured upstream endpoint and credential
    // must never reach the client files or status projection.
    let codex_chat_secret = uuid::Uuid::new_v4().to_string();
    let codex_anthropic_secret = uuid::Uuid::new_v4().to_string();
    let claude_chat_secret = uuid::Uuid::new_v4().to_string();
    let claude_responses_secret = uuid::Uuid::new_v4().to_string();
    let codex_chat = api.create(
        "codex",
        "codex-chat",
        false,
        &codex_chat_secret,
        "chatCompletions",
    );
    let codex_anthropic = api.create(
        "codex",
        "codex-anthropic",
        false,
        &codex_anthropic_secret,
        "anthropicMessages",
    );
    let claude_chat = api.create(
        "claude",
        "claude-chat",
        false,
        &claude_chat_secret,
        "chatCompletions",
    );
    let claude_responses = api.create(
        "claude",
        "claude-responses",
        false,
        &claude_responses_secret,
        "responses",
    );
    for (profile, secret) in [
        (&codex_chat, &codex_chat_secret),
        (&codex_anthropic, &codex_anthropic_secret),
    ] {
        api.switch(profile, secret);
        check_routed_codex(&config, &auth, secret);
        check_active_profile(&api, "codex", profile);
    }
    for (profile, secret) in [
        (&claude_chat, &claude_chat_secret),
        (&claude_responses, &claude_responses_secret),
    ] {
        api.switch(profile, secret);
        check_routed_claude(&settings, secret);
        check_active_profile(&api, "claude", profile);
    }
    // The stored common preference returns as soon as the Codex route no
    // longer needs cross-protocol conversion.
    api.switch(&codex_a, &secret);
    check_codex(&config, &auth, "a", &secret, &oauth, "xhigh");
    check_active_profile(&api, "codex", &codex_a);
    // Cloud restore replaces the whole profile store. It must not invalidate
    // the fingerprints of either active local route before their clients have
    // been switched away from the gateway.
    api.rejected(
        "restore_cloud_backup",
        json!({
            "accountPassword": "unused-in-sandbox",
            "backupPassword": "unused-in-sandbox",
            "confirmWrite": true,
        }),
        "gateway-route-active",
    );

    // Official routes remove custom endpoints/keys while retaining host data and OAuth.
    let official_codex = api.create("codex", "official", true, &secret, "responses");
    let official_claude = api.create("claude", "official", true, &secret, "anthropicMessages");
    api.switch(&official_codex, &secret);
    api.switch(&official_claude, &secret);
    assert!(!read(&config).contains("openai_base_url"));
    let auth_value: Value = serde_json::from_str(&read(&auth)).unwrap();
    assert_eq!(auth_value["auth_mode"], "chatgpt");
    assert!(auth_value["tokens"]["access_token"] == oauth);
    assert!(auth_value["OPENAI_API_KEY"].is_null());
    let claude_value: Value = serde_json::from_str(&read(&settings)).unwrap();
    assert!(claude_value["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
    assert!(claude_value["env"].get("ANTHROPIC_BASE_URL").is_none());
    let backups = api.ok("list_backups", json!({}));
    assert!(!backups.as_array().unwrap().is_empty());
    for backup in backups.as_array().unwrap() {
        assert!(Path::new(backup["backupPath"].as_str().unwrap()).starts_with(root));
    }
    // A pristine installation must create the real files; undo must restore absence.
    for path in [&config, &auth, &settings] {
        fs::remove_file(path).unwrap();
    }
    api.switch(&codex_a, &secret);
    api.switch(&claude_a, &secret);
    assert!(config.exists() && auth.exists() && settings.exists());
    for client in ["codex", "claude"] {
        api.ok(
            "undo_last_switch",
            json!({"target":client,"confirmWrite":true}),
        );
    }
    assert!(!config.exists() && !auth.exists() && !settings.exists());
    if codex != home.join(".codex") {
        assert_eq!(read(&home.join(".codex/config.toml")), "default sentinel");
    }
    if claude != home.join(".claude") {
        assert_eq!(
            read(&home.join(".claude/settings.json")),
            "default sentinel"
        );
    }
}

fn check_codex(
    config: &Path,
    auth: &Path,
    name: &str,
    secret: &str,
    oauth: &str,
    reasoning_effort: &str,
) {
    let text = read(config);
    assert!(text.contains("model_provider = \"openai\""));
    assert!(text.contains(&format!(
        "openai_base_url = \"https://{name}.example.com/v1\""
    )));
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
    assert_eq!(value["auth_mode"], "apikey");
    assert!(value["OPENAI_API_KEY"] == secret);
    assert!(value["tokens"]["access_token"] == oauth);
}

fn check_claude(path: &Path, name: &str, secret: &str) {
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

fn check_routed_codex(config: &Path, auth: &Path, upstream_secret: &str) {
    let text = read(config);
    assert!(text.contains("openai_base_url = \"http://127.0.0.1:"));
    assert!(text.contains("/v1\""));
    assert!(text.contains("web_search = \"disabled\""));
    assert!(!text.contains(upstream_secret));
    let value: Value = serde_json::from_str(&read(auth)).unwrap();
    let token = value["OPENAI_API_KEY"].as_str().unwrap();
    assert!(token.starts_with("asb_local_"));
    assert_ne!(token, upstream_secret);
}

fn check_routed_claude(path: &Path, upstream_secret: &str) {
    let value: Value = serde_json::from_str(&read(path)).unwrap();
    let endpoint = value["env"]["ANTHROPIC_BASE_URL"].as_str().unwrap();
    assert!(endpoint.starts_with("http://127.0.0.1:"));
    assert!(!endpoint.contains("example.com"));
    let token = value["env"]["ANTHROPIC_AUTH_TOKEN"].as_str().unwrap();
    assert!(token.starts_with("asb_local_"));
    assert_ne!(token, upstream_secret);
    assert!(value["env"].get("ANTHROPIC_API_KEY").is_none());
}

fn check_active_profile(api: &Api, client: &str, profile: &Value) {
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
