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
fn client_settings_then_new_providers_switch_through_real_commands() {
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
            "dev_api::sandbox_tests::client_settings_then_new_providers_switch_through_real_commands",
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

mod api;
mod assertions;
mod contracts;
mod fixture;
mod parameters;
mod profiles;
mod recovery;
mod routes;
mod switching;

use api::Api;
use assertions::*;
use fixture::Sandbox;
use profiles::Providers;

fn run_workflow(root: &Path) {
    let sandbox = Sandbox::new(root);
    contracts::gateway_commands(&sandbox);
    contracts::reject_obsolete_credentials(&sandbox);
    let providers = profiles::create(&sandbox);
    profiles::inactive_saves(&sandbox, &providers);
    profiles::preview_and_switch(&sandbox, &providers);
    profiles::active_save(&sandbox, &providers);
    parameters::active_parameter_save(&sandbox, &providers);
    profiles::metadata_save(&sandbox, &providers);
    recovery::unwritten_save(&sandbox, &providers);
    recovery::confirmed_save(&sandbox, &providers);
    switching::claude_active_save(&sandbox, &providers);
    switching::switch_and_undo(&sandbox, &providers);
    switching::stale_parameter_preview(&sandbox, &providers);
    parameters::client_settings_invalidate_preview(&sandbox, &providers);
    parameters::automatic_values_remove_previous_provider_parameters(&sandbox, &providers);
    routes::cross_protocol(&sandbox);
    routes::return_to_original(&sandbox, &providers);
    parameters::subagent_runtime_settings_are_independent(&sandbox, &providers);
    routes::official_routes(&sandbox);
    routes::restore_absence(&sandbox, &providers);
}
