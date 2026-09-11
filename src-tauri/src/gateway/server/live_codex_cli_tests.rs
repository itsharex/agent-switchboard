//! Opt-in closed-loop verification with the real Codex CLI and a real provider.
//!
//! Each route gets its own temporary Codex home, temporary application state
//! and gateway. The client configuration is produced by the shipped switch
//! transaction, and the CLI runs with a cleared environment so it can only see
//! its isolated home.

use super::live_support::{credential, prepare_live_sandbox, LiveRoute, LIVE_ROUTES};
use std::time::Duration;

const CLI_TIMEOUT: Duration = Duration::from_secs(180);

#[test]
#[ignore = "requires ASB_CODEX_LIVE_API_KEY, three protocol-specific upstream URLs and the Codex CLI"]
fn actual_codex_cli_completes_through_every_external_protocol() {
    let api_key = credential();
    let mut failures = Vec::new();
    for route in &LIVE_ROUTES {
        if let Err(failure) = run_cli_route(route, &api_key) {
            failures.push(failure);
        }
    }
    assert!(
        failures.is_empty(),
        "external Codex CLI verification failed:\n{}",
        failures.join("\n")
    );
}

fn run_cli_route(route: &LiveRoute, api_key: &str) -> Result<(), String> {
    let sandbox = prepare_live_sandbox(route, api_key);
    let expected = format!("ASB_CLI_CODEX_{}_OK", route.label.to_ascii_uppercase());
    let output = sandbox.run_codex(
        &format!("Reply with exactly {expected} and no other text."),
        CLI_TIMEOUT,
    );
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let token = sandbox.client_token();
    sandbox.shutdown();
    let safe_stdout = redact(&stdout, api_key, &token);
    let safe_stderr = redact(&stderr, api_key, &token);
    if !output.status.success() {
        return Err(format!(
            "{}: Codex CLI exited with {}; stdout={safe_stdout:?}; stderr={safe_stderr:?}",
            route.label, output.status
        ));
    }
    if !stdout.contains(&expected) {
        return Err(format!(
            "{}: Codex CLI did not return its requested marker; stdout={safe_stdout:?}; stderr={safe_stderr:?}",
            route.label
        ));
    }
    for (name, text) in [("stdout", &stdout), ("stderr", &stderr)] {
        if text.contains(api_key) || text.contains(&token) {
            return Err(format!(
                "{}: Codex CLI {name} exposed a gateway credential",
                route.label
            ));
        }
    }
    Ok(())
}

fn redact(text: &str, api_key: &str, token: &str) -> String {
    text.replace(api_key, "<redacted-provider-key>")
        .replace(token, "<redacted-local-token>")
}
