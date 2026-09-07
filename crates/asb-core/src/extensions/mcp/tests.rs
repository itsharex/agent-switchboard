use super::*;

use std::collections::BTreeMap;

use crate::extensions::contracts::{CodexServerOptions, McpDefinition, SecretValue};
use serde_json::Value as JsonValue;

const CODEX_DOC: &str = r#"# host comment
model = "gpt-5.2"

[mcp_servers.existing]
command = "node"
args = ["server.js"]

[mcp_servers.existing.env]
LOG_LEVEL = "debug"
"#;

fn resolve_yes(_reference: &str) -> Option<String> {
    Some("resolved-secret".to_string())
}

#[test]
fn codex_reader_reports_transport_and_unknown_fields() {
    let servers = read_codex_servers(CODEX_DOC).unwrap();
    assert_eq!(servers.len(), 1);
    let server = &servers[0];
    assert_eq!(server.key, "existing");
    assert_eq!(
        server.transport,
        ObservedTransport::Stdio {
            command: "node".to_string()
        }
    );
    assert!(server.unknown_fields.is_empty());

    let extended = format!(
        "{}\n[mcp_servers.odd]\ncommand = \"x\"\nmystery_field = 1\n",
        CODEX_DOC
    );
    let servers = read_codex_servers(&extended).unwrap();
    let odd = servers.iter().find(|server| server.key == "odd").unwrap();
    assert_eq!(odd.unknown_fields, vec!["mystery_field".to_string()]);
}

#[test]
fn codex_import_preserves_modeled_stdio_and_http_fields() {
    let document = r#"
[mcp_servers.docs]
command = "npx"
args = ["-y", "@example/docs"]
env_vars = ["DOCS_TOKEN"]
cwd = "/work/docs"
startup_timeout_sec = 30
tool_timeout_sec = 90
required = true
enabled = false

[mcp_servers.docs.env]
LOG_LEVEL = "debug"

[mcp_servers.remote]
url = "https://mcp.example.test"
bearer_token_env_var = "REMOTE_TOKEN"
enabled = true

[mcp_servers.remote.http_headers]
X-Client = "switchboard"

[mcp_servers.remote.env_http_headers]
X-Trace = "TRACE_ID"
"#;
    let stdio = import_codex_server(document, "docs").unwrap();
    assert_eq!(
        stdio,
        McpDefinition::Stdio {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@example/docs".to_string()],
            env: BTreeMap::from([
                (
                    "DOCS_TOKEN".to_string(),
                    SecretValue::EnvRef {
                        name: "DOCS_TOKEN".to_string()
                    }
                ),
                (
                    "LOG_LEVEL".to_string(),
                    SecretValue::Plain {
                        value: "debug".to_string()
                    }
                ),
            ]),
            codex_options: Some(CodexServerOptions {
                cwd: Some("/work/docs".to_string()),
                startup_timeout_sec: Some(30),
                tool_timeout_sec: Some(90),
                required: Some(true),
            }),
        }
    );
    let remote = import_codex_server(document, "remote").unwrap();
    assert_eq!(
        remote,
        McpDefinition::Http {
            url: "https://mcp.example.test".to_string(),
            headers: BTreeMap::from([
                (
                    "X-Client".to_string(),
                    SecretValue::Plain {
                        value: "switchboard".to_string()
                    }
                ),
                (
                    "X-Trace".to_string(),
                    SecretValue::EnvRef {
                        name: "TRACE_ID".to_string()
                    }
                ),
            ]),
            bearer: Some(SecretValue::EnvRef {
                name: "REMOTE_TOKEN".to_string()
            }),
        }
    );
}

#[test]
fn native_import_refuses_unmodeled_or_ambiguous_configurations() {
    let codex = r#"[mcp_servers.docs]
command = "npx"
host_option = true
"#;
    assert_eq!(
        import_codex_server(codex, "docs"),
        Err(NativeMcpImportError::UnsupportedFields)
    );
    let conflicting = r#"[mcp_servers.docs]
command = "npx"
env_vars = ["TOKEN"]

[mcp_servers.docs.env]
TOKEN = "literal"
"#;
    assert_eq!(
        import_codex_server(conflicting, "docs"),
        Err(NativeMcpImportError::ConflictingValues)
    );
    let claude = r#"{ "mcpServers": {
  "odd": { "type": "stdio", "command": "npx", "hostOption": true }
} }"#;
    assert_eq!(
        import_claude_server(claude, "odd"),
        Err(NativeMcpImportError::UnsupportedFields)
    );
}

#[test]
fn claude_import_preserves_native_transport_and_environment_references() {
    let document = r#"{ "mcpServers": {
  "docs": {
    "type": "stdio",
    "command": "npx",
    "args": ["-y", "@example/docs"],
    "env": { "DOCS_TOKEN": "${DOCS_TOKEN}", "LOG_LEVEL": "debug" }
  },
  "events": {
    "type": "sse",
    "url": "https://mcp.example.test/sse",
    "headers": { "X-Trace": "${TRACE_ID}" }
  }
} }"#;
    assert_eq!(
        import_claude_server(document, "docs").unwrap(),
        McpDefinition::Stdio {
            command: "npx".to_string(),
            args: vec!["-y".to_string(), "@example/docs".to_string()],
            env: BTreeMap::from([
                (
                    "DOCS_TOKEN".to_string(),
                    SecretValue::EnvRef {
                        name: "DOCS_TOKEN".to_string()
                    }
                ),
                (
                    "LOG_LEVEL".to_string(),
                    SecretValue::Plain {
                        value: "debug".to_string()
                    }
                ),
            ]),
            codex_options: None,
        }
    );
    assert_eq!(
        import_claude_server(document, "events").unwrap(),
        McpDefinition::ClaudeSse {
            url: "https://mcp.example.test/sse".to_string(),
            headers: BTreeMap::from([(
                "X-Trace".to_string(),
                SecretValue::EnvRef {
                    name: "TRACE_ID".to_string()
                }
            )]),
        }
    );
}

#[test]
fn claude_private_project_import_uses_the_project_server_only() {
    let document = r#"{ "mcpServers": {
  "docs": { "type": "stdio", "command": "global" }
}, "projects": {
  "/work/repo": {
    "mcpServers": {
      "docs": { "type": "ws", "url": "wss://mcp.example.test/ws" }
    }
  }
} }"#;
    assert_eq!(
        import_claude_project_private_server(document, "/work/repo", "docs").unwrap(),
        McpDefinition::ClaudeWs {
            url: "wss://mcp.example.test/ws".to_string(),
            headers: BTreeMap::new(),
        }
    );
}

#[test]
fn codex_patches_replace_exactly_one_table_and_keep_the_rest() {
    let render = CodexServerRender::Stdio {
        command: "npx".to_string(),
        args: vec!["-y".to_string(), "docs".to_string()],
        env: BTreeMap::new(),
        env_vars: vec!["DOCS_TOKEN".to_string()],
        options: None,
        enabled: true,
    };
    let (rendered, changes) =
        apply_codex_server_patches(CODEX_DOC, &[("docs".to_string(), Some(render))]).unwrap();
    assert!(rendered.contains("# host comment"));
    assert!(rendered.contains("model = \"gpt-5.2\""));
    assert!(rendered.contains("[mcp_servers.existing]"));
    assert!(rendered.contains("env_vars = [\"DOCS_TOKEN\"]"));
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].pointer, "mcp_servers.docs");
    assert_eq!(changes[0].before, None);

    // Idempotent: applying twice changes nothing further.
    let (twice, second_changes) =
        apply_codex_server_patches(&rendered, &[("docs".to_string(), None)]).unwrap();
    assert!(!twice.contains("[mcp_servers.docs]"));
    assert_eq!(second_changes[0].after, None);
    assert!(twice.contains("[mcp_servers.existing]"));
}

#[test]
fn codex_enable_flag_and_codex_only_options_render() {
    let render = CodexServerRender::Stdio {
        command: "slow".to_string(),
        args: Vec::new(),
        env: BTreeMap::new(),
        env_vars: Vec::new(),
        options: Some(CodexServerOptions {
            cwd: Some("/srv".to_string()),
            startup_timeout_sec: Some(30),
            tool_timeout_sec: None,
            required: Some(true),
        }),
        enabled: false,
    };
    let (rendered, _) =
        apply_codex_server_patches(CODEX_DOC, &[("slow".to_string(), Some(render))]).unwrap();
    assert!(rendered.contains("enabled = false"));
    assert!(rendered.contains("startup_timeout_sec = 30"));
    assert!(rendered.contains("required = true"));
}

#[test]
fn codex_http_render_maps_headers_and_bearer_by_mode() {
    let mut headers = BTreeMap::new();
    headers.insert(
        "X-Static".to_string(),
        SecretValue::Plain {
            value: "plain-value".to_string(),
        },
    );
    headers.insert(
        "X-Traced".to_string(),
        SecretValue::EnvRef {
            name: "TRACE_ID".to_string(),
        },
    );
    let definition = McpDefinition::Http {
        url: "https://mcp.example.com".to_string(),
        headers,
        bearer: Some(SecretValue::EnvRef {
            name: "MCP_TOKEN".to_string(),
        }),
    };
    let render = match render_codex(&definition, true, &resolve_yes).unwrap() {
        CodexServerRender::Http {
            url,
            bearer_token_env_var,
            http_headers,
            env_http_headers,
            ..
        } => (url, bearer_token_env_var, http_headers, env_http_headers),
        other => panic!("expected http render, got {other:?}"),
    };
    assert_eq!(render.0, "https://mcp.example.com");
    assert_eq!(render.1.as_deref(), Some("MCP_TOKEN"));
    assert_eq!(
        render.2.get("X-Static").map(String::as_str),
        Some("plain-value")
    );
    assert_eq!(
        render.3.get("X-Traced").map(String::as_str),
        Some("TRACE_ID")
    );
}

#[test]
fn codex_bearer_rejects_stored_secrets() {
    let definition = McpDefinition::Http {
        url: "https://mcp.example.com".to_string(),
        headers: BTreeMap::new(),
        bearer: Some(SecretValue::SecretRef {
            reference: "ref-1".to_string(),
        }),
    };
    assert!(matches!(
        render_codex(&definition, true, &resolve_yes),
        Err(ProjectionError::BearerNeedsEnvName)
    ));
}

#[test]
fn claude_only_transports_refuse_codex_projection() {
    let definition = McpDefinition::ClaudeSse {
        url: "https://mcp.example.com/sse".to_string(),
        headers: BTreeMap::new(),
    };
    assert!(matches!(
        render_codex(&definition, true, &resolve_yes),
        Err(ProjectionError::Unsupported(_))
    ));
}

#[test]
fn unresolved_secret_references_are_reported_before_any_write() {
    let mut env = BTreeMap::new();
    env.insert(
        "TOKEN".to_string(),
        SecretValue::SecretRef {
            reference: "missing".to_string(),
        },
    );
    let definition = McpDefinition::Stdio {
        command: "npx".to_string(),
        args: Vec::new(),
        env,
        codex_options: None,
    };
    assert!(matches!(
        render_codex(&definition, true, &|_: &str| None),
        Err(ProjectionError::SecretUnavailable(_))
    ));
}

#[test]
fn claude_patches_keep_host_fields_and_render_native_types() {
    let document = r#"{
  "numStartups": 3,
  "mcpServers": {
    "kept": { "type": "stdio", "command": "keep" }
  }
}"#;
    let render = ClaudeServerRender::Stdio {
        command: "npx".to_string(),
        args: vec!["docs".to_string()],
        env: BTreeMap::from([("TOKEN".to_string(), "${MCP_TOKEN}".to_string())]),
    };
    let (rendered, changes) =
        apply_claude_user_server_patches(document, &[("docs".to_string(), Some(render))]).unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    assert_eq!(root["numStartups"], 3);
    assert_eq!(root["mcpServers"]["kept"]["command"], "keep");
    assert_eq!(root["mcpServers"]["docs"]["type"], "stdio");
    assert_eq!(root["mcpServers"]["docs"]["env"]["TOKEN"], "${MCP_TOKEN}");
    assert_eq!(changes.len(), 1);

    // Removal keeps the sibling and the host key.
    let (rendered, _) =
        apply_claude_user_server_patches(&rendered, &[("docs".to_string(), None)]).unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    assert!(root["mcpServers"].get("docs").is_none());
    assert!(root["mcpServers"].get("kept").is_some());
}

#[test]
fn editing_managed_fields_preserves_unknown_fields_inside_the_same_server() {
    // Codex: a host option inside the managed server survives an edit.
    let document = "[mcp_servers.docs]\ncommand = \"old\"\nhost_option = true\n";
    let render = CodexServerRender::Stdio {
        command: "npx".to_string(),
        args: Vec::new(),
        env: BTreeMap::new(),
        env_vars: Vec::new(),
        options: None,
        enabled: true,
    };
    let (rendered, _) =
        apply_codex_server_patches(document, &[("docs".to_string(), Some(render))]).unwrap();
    assert!(rendered.contains("host_option = true"));
    assert!(rendered.contains("command = \"npx\""));

    // Claude: an unknown field inside the managed entry survives too.
    let document = r#"{
  "mcpServers": {
    "docs": { "type": "stdio", "command": "old", "hostOption": true }
  }
}"#;
    let render = ClaudeServerRender::Stdio {
        command: "npx".to_string(),
        args: Vec::new(),
        env: BTreeMap::new(),
    };
    let (rendered, _) =
        apply_claude_user_server_patches(document, &[("docs".to_string(), Some(render))]).unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    assert_eq!(root["mcpServers"]["docs"]["hostOption"], true);
    assert_eq!(root["mcpServers"]["docs"]["command"], "npx");

    // An entry with an unknown transport keeps everything except its
    // shape tag; nothing is silently coerced.
    let odd = r#"{ "mcpServers": { "odd": { "type": "weird", "mystery": 1 } } }"#;
    let render = ClaudeServerRender::Http {
        url: "https://mcp.example.com".to_string(),
        headers: BTreeMap::new(),
    };
    let (rendered, _) =
        apply_claude_user_server_patches(odd, &[("odd".to_string(), Some(render))]).unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    assert_eq!(root["mcpServers"]["odd"]["mystery"], 1);
    assert_eq!(root["mcpServers"]["odd"]["type"], "http");
}

#[test]
fn claude_http_bearer_renders_into_authorization() {
    let definition = McpDefinition::Http {
        url: "https://mcp.example.com".to_string(),
        headers: BTreeMap::new(),
        bearer: Some(SecretValue::SecretRef {
            reference: "ref-1".to_string(),
        }),
    };
    let render = render_claude(&definition, &resolve_yes).unwrap();
    match render {
        ClaudeServerRender::Http { headers, .. } => {
            assert_eq!(
                headers.get("Authorization").map(String::as_str),
                Some("Bearer resolved-secret")
            );
        }
        other => panic!("expected http render, got {other:?}"),
    }
}

#[test]
fn claude_project_private_patches_nest_under_the_project_path() {
    let document = r#"{ "projects": { "/work/repo": { "allowedTools": ["Read"] } } }"#;
    let render = ClaudeServerRender::Stdio {
        command: "srv".to_string(),
        args: Vec::new(),
        env: BTreeMap::new(),
    };
    let (rendered, changes) = apply_claude_project_private_server_patches(
        document,
        "/work/repo",
        &[("private".to_string(), Some(render))],
    )
    .unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    assert_eq!(
        root["projects"]["/work/repo"]["mcpServers"]["private"]["command"],
        "srv"
    );
    assert_eq!(changes[0].pointer, "projects./work/repo.mcpServers.private");
    assert_eq!(root["projects"]["/work/repo"]["allowedTools"][0], "Read");
}

#[test]
fn disabled_members_are_added_and_removed_individually() {
    let document = r#"{}"#;
    let (rendered, _) =
        apply_claude_project_disabled_members(document, "/work/repo", &["shared".to_string()], &[])
            .unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    assert_eq!(
        root["projects"]["/work/repo"]["disabledMcpServers"][0],
        "shared"
    );

    let (rendered, changes) = apply_claude_project_disabled_members(
        &rendered,
        "/work/repo",
        &[],
        &["shared".to_string()],
    )
    .unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    // The empty collection is cleaned up with the member.
    assert!(root["projects"]["/work/repo"]
        .get("disabledMcpServers")
        .is_none());
    assert_eq!(changes.len(), 1);
}

#[test]
fn private_disabled_member_preserves_project_server_and_native_members() {
    let document = r#"{
  "projects": {
    "/work/repo": {
      "allowedTools": ["Read"],
      "mcpServers": {
        "private": { "type": "stdio", "command": "srv" }
      },
      "disabledMcpServers": ["private", "other"]
    }
  }
}"#;
    assert!(claude_project_disabled_member_present(document, "/work/repo", "private").unwrap());
    let (rendered, changes) = apply_claude_project_disabled_members(
        document,
        "/work/repo",
        &[],
        &["private".to_string()],
    )
    .unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    assert_eq!(
        root["projects"]["/work/repo"]["mcpServers"]["private"]["command"],
        "srv"
    );
    assert_eq!(root["projects"]["/work/repo"]["allowedTools"][0], "Read");
    assert_eq!(
        root["projects"]["/work/repo"]["disabledMcpServers"],
        JsonValue::Array(vec![JsonValue::String("other".to_string())])
    );
    assert_eq!(changes.len(), 1);
    assert!(!claude_project_disabled_member_present(&rendered, "/work/repo", "private").unwrap());

    let (_, no_change) =
        apply_claude_project_disabled_members(&rendered, "/work/repo", &["other".to_string()], &[])
            .unwrap();
    assert!(no_change.is_empty());
}

#[test]
fn private_disabled_member_rejects_malformed_native_containers() {
    let malformed = r#"{ "projects": [] }"#;
    assert!(claude_project_disabled_member_present(malformed, "/work/repo", "private").is_err());
    assert!(apply_claude_project_disabled_members(
        malformed,
        "/work/repo",
        &["private".to_string()],
        &[]
    )
    .is_err());
}

#[test]
fn skill_overrides_round_trip_through_the_four_documented_states() {
    let document = r#"{ "model": "claude-sonnet-5" }"#;
    let (rendered, changes) = apply_claude_skill_overrides(
        document,
        &[("helper".to_string(), Some(SkillOverrideValue::Off))],
    )
    .unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    assert_eq!(root["skillOverrides"]["helper"], "off");
    assert_eq!(root["model"], "claude-sonnet-5");
    assert_eq!(changes[0].before, None);

    // Removing our own override cleans up the empty collection.
    let (rendered, changes) =
        apply_claude_skill_overrides(&rendered, &[("helper".to_string(), None)]).unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    assert!(root.get("skillOverrides").is_none());
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].before.as_deref(), Some("\"off\""));

    // Existing advanced states are visible to readers.
    let advanced = r#"{ "skillOverrides": { "helper": "name-only" } }"#;
    let root: JsonValue = serde_json::from_str(advanced).unwrap();
    let current = root["skillOverrides"]["helper"].as_str().unwrap();
    assert_eq!(
        SkillOverrideValue::parse_native(current),
        Some(SkillOverrideValue::NameOnly)
    );
}

#[test]
fn codex_skill_rules_replace_by_path_and_clean_up() {
    let document = "model = \"gpt-5.2\"\n";
    let rule_path = "/home/user/.agents/skills/api-spec/SKILL.md";
    let (rendered, changes) =
        apply_codex_skill_rules(document, &[(rule_path.to_string(), Some(false))]).unwrap();
    assert!(rendered.contains("[[skills.config]]"));
    assert!(rendered.contains("enabled = false"));
    assert_eq!(changes.len(), 1);

    let rules = read_codex_skill_rules(&rendered).unwrap();
    assert_eq!(
        rules,
        vec![CodexSkillRule {
            path: rule_path.to_string(),
            enabled: false
        }]
    );

    // Removing the owned rule removes the empty structure too.
    let (rendered, _) =
        apply_codex_skill_rules(&rendered, &[(rule_path.to_string(), None)]).unwrap();
    assert!(!rendered.contains("skills"));
    assert_eq!(rendered, document);
}

#[test]
fn skill_rule_restores_preserve_exact_native_state() {
    let rule_path = "/home/user/.agents/skills/api-spec/SKILL.md";
    let codex_before = format!(
        "[[skills.config]]\npath = \"{rule_path}\"\nenabled = true\nnative_flag = \"keep\"\n"
    );
    let (codex_disabled, codex_changes) =
        apply_codex_skill_rules(&codex_before, &[(rule_path.to_string(), Some(false))]).unwrap();
    assert!(codex_disabled.contains("native_flag = \"keep\""));
    let codex_original = codex_changes[0].before.as_deref();
    let (codex_restored, _) =
        apply_codex_skill_rule_restore(&codex_disabled, rule_path, codex_original).unwrap();
    assert_eq!(codex_restored, codex_before);

    let claude_before = r#"{ "skillOverrides": { "helper": "name-only" } }"#;
    let (claude_disabled, claude_changes) = apply_claude_skill_overrides(
        claude_before,
        &[("helper".to_string(), Some(SkillOverrideValue::Off))],
    )
    .unwrap();
    let claude_original = claude_changes[0].before.as_deref();
    let (claude_restored, _) =
        apply_claude_skill_override_restore(&claude_disabled, "helper", claude_original).unwrap();
    let root: JsonValue = serde_json::from_str(&claude_restored).unwrap();
    assert_eq!(root["skillOverrides"]["helper"], "name-only");
}

#[test]
fn redaction_masks_connection_material_in_rendered_entries() {
    let entry = "bearer = \"sk-live-0123456789abcdef\"\nurl = \"https://private.example.test/mcp\"\nargs = [\"--api-key\", \"private-argument\"]\n";
    let redacted = redact_rendered_entry(entry);
    assert!(redacted.contains(&*crate::redact::REDACTED));
    assert!(!redacted.contains("private.example.test"));
    assert!(!redacted.contains("private-argument"));

    let json = r#"{"url":"https://private.example.test/mcp","headers":{"X-Key":"private-header"},"args":["private-argument"]}"#;
    let redacted_json = redact_rendered_entry(json);
    assert!(!redacted_json.contains("private.example.test"));
    assert!(!redacted_json.contains("private-header"));
    assert!(!redacted_json.contains("private-argument"));
}

// ---------------------------------------------------------- key renames

fn codex_stdio_render(command: &str) -> CodexServerRender {
    CodexServerRender::Stdio {
        command: command.to_string(),
        args: Vec::new(),
        env: BTreeMap::new(),
        env_vars: Vec::new(),
        options: None,
        enabled: true,
    }
}

fn claude_stdio_render(command: &str) -> ClaudeServerRender {
    ClaudeServerRender::Stdio {
        command: command.to_string(),
        args: Vec::new(),
        env: BTreeMap::new(),
    }
}

#[test]
fn codex_rename_patch_removes_the_old_key_and_writes_the_new_one() {
    let (rendered, changes) = apply_codex_server_patches(
        CODEX_DOC,
        &[
            ("existing".to_string(), None),
            ("renamed".to_string(), Some(codex_stdio_render("node"))),
        ],
    )
    .unwrap();
    assert!(!rendered.contains("mcp_servers.existing"));
    assert!(rendered.contains("mcp_servers.renamed"));
    // Host-owned content outside the entry survives the move.
    assert!(rendered.contains("# host comment"));
    assert!(rendered.contains("gpt-5.2"));
    let removed = changes
        .iter()
        .find(|change| change.pointer == "mcp_servers.existing")
        .expect("old key removed");
    assert!(removed.before.is_some() && removed.after.is_none());
    let added = changes
        .iter()
        .find(|change| change.pointer == "mcp_servers.renamed")
        .expect("new key written");
    assert!(added.before.is_none() && added.after.is_some());
}

#[test]
fn claude_user_rename_patch_moves_the_entry_and_keeps_neighbours() {
    let document = r#"{ "mcpServers": { "docs": { "command": "srv" }, "other": { "command": "keep" } }, "host": true }"#;
    let (rendered, changes) = apply_claude_user_server_patches(
        document,
        &[
            ("docs".to_string(), None),
            ("docs_v2".to_string(), Some(claude_stdio_render("srv"))),
        ],
    )
    .unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    assert!(root["mcpServers"].get("docs").is_none());
    assert_eq!(root["mcpServers"]["docs_v2"]["command"], "srv");
    assert_eq!(root["mcpServers"]["other"]["command"], "keep");
    assert_eq!(root["host"], serde_json::json!(true));
    assert_eq!(changes.len(), 2);
}

#[test]
fn claude_private_rename_moves_the_disable_member_with_the_key() {
    let document = r#"{ "projects": { "/work/repo": { "disabledMcpServers": ["docs"], "allowedTools": ["Read"] } } }"#;
    // A disabled binding renames with no server entries on disk: only
    // the disable member moves.
    let (after_servers, server_changes) = apply_claude_project_private_server_patches(
        document,
        "/work/repo",
        &[("docs".to_string(), None), ("docs_v2".to_string(), None)],
    )
    .unwrap();
    assert!(server_changes.is_empty());
    let (rendered, member_changes) = apply_claude_project_disabled_members(
        &after_servers,
        "/work/repo",
        &["docs_v2".to_string()],
        &["docs".to_string()],
    )
    .unwrap();
    let root: JsonValue = serde_json::from_str(&rendered).unwrap();
    let members = root["projects"]["/work/repo"]["disabledMcpServers"]
        .as_array()
        .expect("member array");
    assert_eq!(members, &["docs_v2".to_string()]);
    assert_eq!(root["projects"]["/work/repo"]["allowedTools"][0], "Read");
    assert_eq!(member_changes.len(), 2);
}
