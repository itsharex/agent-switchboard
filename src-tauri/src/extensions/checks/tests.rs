#![cfg(test)]

use super::stdio::{read_line_capped, rpc_request};
use super::*;
use std::io::BufReader;

use std::io::Cursor;

use asb_core::contracts::AppKind;

#[test]
fn static_diagnostics_report_missing_commands_without_running_anything() {
    let diagnostics = static_diagnostics(&McpDefinition::Stdio {
        command: "definitely-not-on-path-0123456789".to_string(),
        args: Vec::new(),
        env: Default::default(),
        codex_options: None,
    });
    assert_eq!(diagnostics.len(), 1);
    assert!(diagnostics[0].contains("未在 PATH"));
}

#[test]
fn material_resolves_environment_references_for_the_actual_probe() {
    let definition = McpDefinition::Stdio {
        command: "cmd".to_string(),
        args: Vec::new(),
        env: BTreeMap::from([(
            "CHECK_PATH".to_string(),
            SecretValue::EnvRef {
                name: "PATH".to_string(),
            },
        )]),
        codex_options: None,
    };
    let material = material_for(&definition, &|_| None).unwrap();
    assert_eq!(
        material.env.get("CHECK_PATH"),
        Some(&std::env::var("PATH").unwrap())
    );
}

#[test]
fn capped_line_reader_rejects_unbounded_frames() {
    let mut reader = BufReader::new(Cursor::new(b"12345\\n".to_vec()));
    assert!(read_line_capped(&mut reader, 4).is_err());
}

#[test]
fn modern_requests_carry_the_required_metadata() {
    let request = rpc_request(1, "tools/list", None, true);
    let meta = &request["params"]["_meta"];
    assert_eq!(
        meta["io.modelcontextprotocol/protocolVersion"],
        MODERN_PROTOCOL_VERSION
    );
    assert!(meta["io.modelcontextprotocol/clientCapabilities"].is_object());
}

#[test]
fn stdio_check_against_a_real_local_process_reports_outcomes() {
    let registry = CheckRegistry::new();
    let (check_id, _flag) = registry.start();
    let definition = McpDefinition::Stdio {
        command: if cfg!(windows) {
            "cmd".to_string()
        } else {
            "sh".to_string()
        },
        args: vec![
            if cfg!(windows) {
                "/c".to_string()
            } else {
                "-c".to_string()
            },
            "exit 0".to_string(),
        ],
        env: Default::default(),
        codex_options: None,
    };
    let result = run_check_for(
        "r-1",
        1,
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        &check_id,
        &registry,
        &definition,
        &|_| None,
    );
    assert!(matches!(
        result.outcome,
        McpCheckOutcome::Failed { classification, .. } if classification == "protocol"
    ));
}

#[test]
fn cancellation_stops_the_probe() {
    let registry = CheckRegistry::new();
    let (check_id, flag) = registry.start();
    flag.store(true, Ordering::SeqCst);
    let definition = McpDefinition::Stdio {
        command: if cfg!(windows) {
            "cmd".to_string()
        } else {
            "sleep".to_string()
        },
        args: vec![if cfg!(windows) {
            "/c pause".to_string()
        } else {
            "30".to_string()
        }],
        env: Default::default(),
        codex_options: None,
    };
    let result = run_check_for(
        "r-1",
        1,
        &ExtensionTarget::App {
            client: AppKind::Codex,
        },
        &check_id,
        &registry,
        &definition,
        &|_| None,
    );
    assert!(matches!(result.outcome, McpCheckOutcome::Cancelled));
}
