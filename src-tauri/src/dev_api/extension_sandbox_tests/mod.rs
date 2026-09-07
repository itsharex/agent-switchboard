//! Black-box HTTP tests for the extensions workspace commands: local skill
//! authoring, immutable content versions, dependency links, and the combined
//! deployment flow. Each child process owns a temporary home, client
//! directories, and Tauri app data; nothing reads or writes real client
//! configuration or real credentials.

use super::*;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};
use tauri::Manager;

const CHILD_ROOT: &str = "ASB_EXTENSION_TEST_ROOT";
const ORIGIN: &str = "http://127.0.0.1:1420";

/// Runs one test body inside an isolated child process and asserts success.
/// The parent redirects USERPROFILE/HOME/APPDATA and clears client roots;
/// the body receives the sandbox root.
fn isolated(name: &str, body: fn(&Path)) {
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        body(Path::new(&root));
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let home = root.join("home");
    fs::create_dir_all(home.join("AppData/Roaming")).unwrap();
    fs::create_dir_all(home.join("AppData/Local")).unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command.args(["--exact", name, "--nocapture"]);
    command
        .env(CHILD_ROOT, root)
        .env("USERPROFILE", &home)
        .env("HOME", &home)
        .env("APPDATA", home.join("AppData/Roaming"))
        .env("LOCALAPPDATA", home.join("AppData/Local"))
        .env_remove("CODEX_HOME")
        .env_remove("CLAUDE_CONFIG_DIR");
    let mut child = command.spawn().unwrap();
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "extension sandbox child failed: {name}");
            break;
        }
        if started.elapsed() > Duration::from_secs(120) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("extension sandbox test timed out: {name}");
        }
        std::thread::sleep(Duration::from_millis(25));
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
            .expect("extension sandbox HTTP request");
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

    fn rejected(&self, command: &str, args: Value, code: &str) -> Value {
        let response = self.request(command, args);
        assert_eq!(response["kind"], "failure", "{command} must reject");
        assert_eq!(response["error"]["code"], code, "{command} rejection");
        response["error"].clone()
    }
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, text).unwrap();
}

fn write_bytes(path: &Path, bytes: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, bytes).unwrap();
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap()
}

fn start_api(root: &Path) -> Api {
    let mut context = tauri::generate_context!();
    context.config_mut().app.windows.clear();
    context.config_mut().identifier = root.join("app-data").to_string_lossy().into_owned();
    let app = tauri::Builder::default()
        .any_thread()
        .build(context)
        .unwrap();
    assert_eq!(app.path().app_data_dir().unwrap(), root.join("app-data"));
    let state = crate::local_state::LocalState::from_app(app.handle()).unwrap();
    app.manage(crate::gateway::GatewayController::start(&state).unwrap());
    app.manage(crate::commands::switching::ProfileSavePreparations::default());
    let server = Server::http("127.0.0.1:0").unwrap();
    let url = format!("http://{}/invoke", server.server_addr());
    let handle = app.handle().clone();
    std::thread::Builder::new()
        .name("asb-extension-test-api".to_string())
        .spawn(move || serve(server, handle, ORIGIN.to_string()))
        .unwrap();
    Api {
        url,
        client: reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap(),
    }
}

fn editor_file<'a>(editor: &'a Value, path: &str) -> &'a Value {
    editor["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| file["relativePath"] == path)
        .unwrap_or_else(|| panic!("editor lacks {path}"))
}

// ------------------------------------------------------------ authoring flow

mod authoring;
mod deployment;
mod migration;
mod portable;
mod takeover;
