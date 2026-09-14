//! Real command/HTTP coverage. A child process owns every filesystem root;
//! the only upstream is an ephemeral loopback server with dummy credentials.

use super::{serve, Server};
use asb_core::contracts::{AppKind, ProviderDraft};
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use tauri::Manager;

const CHILD_ROOT: &str = "ASB_PROVIDER_REQUEST_TEST_ROOT";
const ORIGIN: &str = "http://127.0.0.1:1420";
const TEST_NAME: &str =
    "dev_api::provider_request_tests::requests_complete_and_cancel_through_browser_commands";

#[test]
fn requests_complete_and_cancel_through_browser_commands() {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        run_workflow(Path::new(&root));
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let home = root.join("home");
    fs::create_dir_all(home.join("AppData/Roaming")).unwrap();
    fs::create_dir_all(home.join("AppData/Local")).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(CHILD_ROOT, root)
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .env("APPDATA", home.join("AppData/Roaming"))
        .env("LOCALAPPDATA", home.join("AppData/Local"))
        .env("CODEX_HOME", root.join("codex"))
        .env("CLAUDE_CONFIG_DIR", root.join("claude"))
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .spawn()
        .unwrap();
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                status.success(),
                "isolated provider request workflow failed"
            );
            break;
        }
        if started.elapsed() > Duration::from_secs(30) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("isolated provider request workflow timed out");
        }
        thread::sleep(Duration::from_millis(25));
    }
}

struct Api {
    url: String,
    client: reqwest::blocking::Client,
}

impl Api {
    fn invoke(&self, command: &str, args: Value) -> Value {
        let response = self
            .client
            .post(&self.url)
            .header("Origin", ORIGIN)
            .header("Content-Type", "application/json")
            .body(json!({ "command": command, "args": args }).to_string())
            .send()
            .expect("loopback command response");
        assert_eq!(response.status(), 200);
        serde_json::from_str(&response.text().unwrap()).unwrap()
    }

    fn ok(&self, command: &str, args: Value) -> Value {
        let response = self.invoke(command, args);
        assert_eq!(
            response["kind"], "success",
            "{command}: {}",
            response["error"]
        );
        response["result"].clone()
    }
}

fn start_api(root: &Path) -> (tauri::App, crate::local_state::LocalState, Api) {
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    context.config_mut().identifier = root.join("app-data").to_string_lossy().into_owned();
    let app = tauri::Builder::default()
        .any_thread()
        .build(context)
        .unwrap();
    assert_eq!(app.path().app_data_dir().unwrap(), root.join("app-data"));
    let state = crate::local_state::LocalState::from_app(app.handle()).unwrap();
    state.initialize_schemas().unwrap();
    app.manage(crate::provider_request::ProviderRequests::default());
    let server = Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/invoke", server.server_addr());
    let handle = app.handle().clone();
    thread::spawn(move || serve(server, handle, ORIGIN.to_string()));
    let api = Api {
        url,
        client: reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(4))
            .build()
            .unwrap(),
    };
    (app, state, api)
}

fn draft(base_url: &str) -> ProviderDraft {
    serde_json::from_value(json!({
        "app": "claude", "routeMode": "custom", "name": "Loopback request fixture",
        "apiKey": "isolated-provider-request-key", "baseUrl": base_url,
        "model": "stored-model", "upstreamProtocol": "responses",
        "responsesOptions": { "requestMode": "standard" },
        "maxOutputTokens": null, "modelOptions": null, "websiteUrl": null,
        "parameters": asb_core::ownership::default_provider_parameters(AppKind::Claude),
    }))
    .unwrap()
}

fn run_workflow(root: &Path) {
    let sentinels = [
        root.join("codex/config.toml"),
        root.join("codex/auth.json"),
        root.join("claude/settings.json"),
    ];
    for path in &sentinels {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "untouched client fixture").unwrap();
    }
    let (_app, state, api) = start_api(root);
    let upstream = Server::http("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v1", upstream.server_addr());
    let record = state
        .configuration()
        .create_provider(draft(&base_url))
        .unwrap();
    let profile_id = &record.profile.id;
    prepares_unsaved_draft(&api, &upstream, &base_url);
    completes(&api, &upstream, profile_id, &base_url);
    fetches_models(&api, &upstream, profile_id, &base_url);
    cancels_without_waiting_for_upstream(&api, &upstream, profile_id);
    rejects_cancelled_preparation(&api, &upstream, profile_id);
    assert_eq!(
        state
            .configuration()
            .find_provider_record(profile_id)
            .unwrap()
            .file_hash,
        record.file_hash
    );
    rejects_changed_profile(&api, &state, profile_id, &base_url);
    for path in &sentinels {
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            "untouched client fixture"
        );
    }
}

fn prepares(api: &Api, upstream: &Server, profile_id: &str) -> Value {
    let preparation = api.ok(
        "prepare_provider_request",
        json!({"target": {"kind": "saved", "profileId": profile_id}}),
    );
    assert!(!preparation
        .to_string()
        .contains("isolated-provider-request-key"));
    assert!(upstream
        .recv_timeout(Duration::from_millis(50))
        .unwrap()
        .is_none());
    preparation
}

fn prepares_unsaved_draft(api: &Api, upstream: &Server, base_url: &str) {
    let preparation = api.ok(
        "prepare_provider_request",
        json!({"target": {
            "kind":"draft", "connection": {
                "app":"claude", "baseUrl":base_url, "apiKey":"isolated-unsaved-key", "upstreamProtocol":"responses",
                "responsesOptions":{"requestMode":"minimal"}, "defaultModel":"unsaved-model"
            }
        }}),
    );
    assert_eq!(preparation["endpoint"], format!("{base_url}/responses"));
    assert_eq!(preparation["defaultModel"], "unsaved-model");
    assert!(!preparation.to_string().contains("isolated-unsaved-key"));
    assert!(upstream
        .recv_timeout(Duration::from_millis(50))
        .unwrap()
        .is_none());
    assert_eq!(
        api.ok(
            "cancel_provider_request",
            json!({"requestId":preparation["requestId"]})
        ),
        true
    );
    let old = api.invoke("prepare_provider_request", json!({"profileId":"obsolete"}));
    assert_eq!(old["kind"], "failure");
}

fn completes(api: &Api, upstream: &Server, profile_id: &str, base_url: &str) {
    let preparation = prepares(api, upstream, profile_id);
    assert_eq!(preparation["endpoint"], format!("{base_url}/responses"));
    assert_eq!(preparation["defaultModel"], "stored-model");
    assert_eq!(preparation["prompt"], "请只回复：连接成功。");
    let result = thread::scope(|scope| {
        let execution =
            scope.spawn(|| {
                api.ok("execute_provider_request", json!({
            "request": {"requestId": preparation["requestId"], "model": "temporary-model"}
        }))
            });
        let mut request = upstream
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap();
        assert_eq!(request.method(), &tiny_http::Method::Post);
        assert_eq!(request.url(), "/v1/responses");
        assert!(request
            .headers()
            .iter()
            .any(|header| header.field.equiv("Authorization")
                && header.value.as_str() == "Bearer isolated-provider-request-key"));
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).unwrap();
        let payload: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(payload["model"], "temporary-model");
        assert!(body.contains("请只回复：连接成功。"));
        let response = json!({
            "id": "response-fixture", "object": "response", "status": "completed",
            "model": "returned-model", "output": [{"type": "message", "role": "assistant",
                "status": "completed", "content": [{"type": "output_text", "text": "连接成功。"}]}]
        });
        request
            .respond(
                tiny_http::Response::from_string(response.to_string()).with_header(
                    tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
                ),
            )
            .unwrap();
        execution.join().unwrap()
    });
    assert_eq!(result["outcome"], "success");
    assert_eq!(result["status"], 200);
    assert_eq!(result["reply"], "连接成功。");
    assert_eq!(result["model"], "returned-model");
    assert!(!result.to_string().contains("isolated-provider-request-key"));
    let obsolete = api.invoke("execute_provider_request", json!({
        "request": {"requestId": preparation["requestId"], "model": "temporary-model", "apiKey": "not-accepted"}
    }));
    assert_eq!(obsolete["kind"], "failure");
    assert_eq!(obsolete["error"]["code"], "web-argument-invalid");
}

fn fetches_models(api: &Api, upstream: &Server, profile_id: &str, base_url: &str) {
    let preparation = prepares(api, upstream, profile_id);
    let request_id = preparation["requestId"].clone();
    let expected = json!([
        { "id": "loopback-model", "ownedBy": "fixture-vendor" },
        { "id": "bare-model", "ownedBy": null },
    ]);
    let saved = list_models(
        api,
        upstream,
        &request_id,
        "Bearer isolated-provider-request-key",
    );
    assert_eq!(saved, expected);
    assert!(!saved.to_string().contains("isolated-provider-request-key"));

    let obsolete = api.invoke(
        "fetch_provider_request_models",
        json!({"target": {"kind": "draft", "connection": {"apiKey": "not-accepted"}}}),
    );
    assert_eq!(obsolete["kind"], "failure");
    assert_eq!(obsolete["error"]["code"], "web-argument-missing");
    assert!(upstream
        .recv_timeout(Duration::from_millis(50))
        .unwrap()
        .is_none());

    // A listing reads the credential from the prepared request and must not
    // retire the token the pending request still needs.
    let repeated = list_models(
        api,
        upstream,
        &request_id,
        "Bearer isolated-provider-request-key",
    );
    assert_eq!(repeated, expected);
    assert_eq!(
        api.ok(
            "cancel_provider_request",
            json!({ "requestId": request_id })
        ),
        true
    );
    let retired = api.invoke(
        "fetch_provider_request_models",
        json!({ "requestId": request_id }),
    );
    assert_eq!(retired["kind"], "failure");
    assert!(upstream
        .recv_timeout(Duration::from_millis(50))
        .unwrap()
        .is_none());

    let draft_preparation = api.ok(
        "prepare_provider_request",
        json!({"target": {
            "kind":"draft", "connection": {
                "app":"claude", "baseUrl":base_url, "apiKey":"isolated-unsaved-key", "upstreamProtocol":"responses",
                "responsesOptions":{"requestMode":"minimal"}, "defaultModel":"unsaved-model"
            }
        }}),
    );
    let draft = list_models(
        api,
        upstream,
        &draft_preparation["requestId"],
        "Bearer isolated-unsaved-key",
    );
    assert_eq!(draft[0]["id"], "loopback-model");
    assert!(!draft.to_string().contains("isolated-unsaved-key"));
    assert_eq!(
        api.ok(
            "cancel_provider_request",
            json!({ "requestId": draft_preparation["requestId"] })
        ),
        true
    );
}

fn list_models(api: &Api, upstream: &Server, request_id: &Value, authorization: &str) -> Value {
    thread::scope(|scope| {
        let listing = scope.spawn(|| {
            api.ok(
                "fetch_provider_request_models",
                json!({ "requestId": request_id }),
            )
        });
        answer_models(upstream, authorization);
        listing.join().unwrap()
    })
}

fn answer_models(upstream: &Server, authorization: &str) {
    let request = upstream
        .recv_timeout(Duration::from_secs(3))
        .unwrap()
        .unwrap();
    assert_eq!(request.method(), &tiny_http::Method::Get);
    assert_eq!(request.url(), "/v1/models");
    assert!(request.headers().iter().any(
        |header| header.field.equiv("Authorization") && header.value.as_str() == authorization
    ));
    let response = json!({
        "data": [
            { "id": "loopback-model", "owned_by": "fixture-vendor" },
            { "id": "bare-model" }
        ]
    });
    request
        .respond(
            tiny_http::Response::from_string(response.to_string()).with_header(
                tiny_http::Header::from_bytes("Content-Type", "application/json").unwrap(),
            ),
        )
        .unwrap();
}

fn cancels_without_waiting_for_upstream(api: &Api, upstream: &Server, profile_id: &str) {
    let preparation = prepares(api, upstream, profile_id);
    thread::scope(|scope| {
        let execution =
            scope.spawn(|| {
                api.ok("execute_provider_request", json!({
            "request": {"requestId": preparation["requestId"], "model": "temporary-model"}
        }))
            });
        let pending = upstream
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap();
        // Holding the upstream request open proves cancellation travels through
        // the bridge concurrently with the pending execute command.
        let cancelled = api.ok(
            "cancel_provider_request",
            json!({"requestId": preparation["requestId"]}),
        );
        assert_eq!(cancelled, true);
        assert_eq!(execution.join().unwrap()["outcome"], "cancelled");
        drop(pending);
    });
}

fn rejects_cancelled_preparation(api: &Api, upstream: &Server, profile_id: &str) {
    let preparation = prepares(api, upstream, profile_id);
    assert_eq!(
        api.ok(
            "cancel_provider_request",
            json!({"requestId": preparation["requestId"]})
        ),
        true
    );
    let result = api.invoke(
        "execute_provider_request",
        json!({
            "request": {"requestId": preparation["requestId"], "model": "temporary-model"}
        }),
    );
    assert_eq!(result["kind"], "failure");
    assert!(upstream
        .recv_timeout(Duration::from_millis(50))
        .unwrap()
        .is_none());
}

fn rejects_changed_profile(
    api: &Api,
    state: &crate::local_state::LocalState,
    profile_id: &str,
    base_url: &str,
) {
    let preparation = api.ok(
        "prepare_provider_request",
        json!({"target": {"kind": "saved", "profileId": profile_id}}),
    );
    let record = state
        .configuration()
        .find_provider_record(profile_id)
        .unwrap();
    let mut changed = draft(base_url);
    changed.api_key = "changed-isolated-key".to_string();
    state
        .configuration()
        .update_provider(profile_id, changed, &record.file_hash)
        .unwrap();
    let result = api.invoke(
        "execute_provider_request",
        json!({
            "request": {"requestId": preparation["requestId"], "model": "temporary-model"}
        }),
    );
    assert_eq!(result["kind"], "failure");
    assert!(!result.to_string().contains("changed-isolated-key"));
}
