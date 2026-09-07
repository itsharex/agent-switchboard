use super::*;
use crate::contracts::ModelOptions;
use crate::test_support::{CLAUDE_JSON, CODEX_AUTH_JSON, CODEX_TOML};

#[test]
fn missing_files_are_reported_not_guessed() {
    let report = discover(
        &DiscoveryPaths {
            codex: "~/.codex/config.toml".into(),
            codex_auth: "~/.codex/auth.json".into(),
            claude: "~/.claude/settings.json".into(),
        },
        |_| Ok(None),
    );
    assert_eq!(report.codex.state, DiscoveredState::Missing);
    assert_eq!(report.claude.state, DiscoveredState::Missing);
    assert!(report.import_proposals.is_empty());
}

#[test]
fn parse_errors_carry_line_and_scrubbed_message() {
    let report = discover(
        &DiscoveryPaths {
            codex: "c".into(),
            codex_auth: "a".into(),
            claude: "s".into(),
        },
        |p| {
            if p == "c" {
                Ok(Some("broken [".into()))
            } else {
                Ok(None)
            }
        },
    );
    match report.codex.state {
        DiscoveredState::ParseError { line, .. } => assert!(line.is_some()),
        other => panic!("expected parse error, got {other:?}"),
    }
}

#[test]
fn test_configuration_produces_routes_and_imports_api_keys() {
    let report = discover(
        &DiscoveryPaths {
            codex: "c".into(),
            codex_auth: "a".into(),
            claude: "s".into(),
        },
        |p| {
            if p == "c" {
                Ok(Some(CODEX_TOML.to_string()))
            } else if p == "a" {
                Ok(Some(CODEX_AUTH_JSON.to_string()))
            } else {
                Ok(Some(CLAUDE_JSON.to_string()))
            }
        },
    );
    let DiscoveredState::Ok { route, managed, .. } = &report.codex.state else {
        panic!("codex should be ok");
    };
    assert!(managed);
    assert_eq!(route.route_mode, RouteMode::Custom);
    assert_eq!(route.provider_name, None);
    assert_eq!(route.wire_api, None);
    let codex_proposal = report
        .import_proposals
        .iter()
        .find(|proposal| proposal.app == AppKind::Codex)
        .expect("codex proposal");
    assert_eq!(codex_proposal.draft.api_key, "TEST_CODEX_IMPORT_KEY");
    let claude_proposal = report
        .import_proposals
        .iter()
        .find(|proposal| proposal.app == AppKind::Claude)
        .expect("claude proposal");
    assert_eq!(claude_proposal.draft.api_key, "TEST_CLAUDE_IMPORT_KEY");

    // Only the cache copy is redacted; the live report keeps its keys.
    let cached = report.cached_display();
    assert_eq!(cached.codex, report.codex);
    assert_eq!(cached.claude, report.claude);
    for proposal in &cached.import_proposals {
        assert_eq!(proposal.draft.api_key, "");
    }
    for proposal in &report.import_proposals {
        assert!(!proposal.draft.api_key.is_empty());
    }
}

#[test]
fn importing_claude_configuration_decodes_lowercase_one_m_into_semantic_state() {
    let current = r#"{"env":{"ANTHROPIC_BASE_URL":"https://relay.internal","ANTHROPIC_AUTH_TOKEN":"TEST_CLAUDE_IMPORT_KEY","ANTHROPIC_MODEL":"claude-opus-4-1[1m]","ANTHROPIC_DEFAULT_SONNET_MODEL":"claude-sonnet-4-6[1m]","ANTHROPIC_DEFAULT_OPUS_MODEL":"claude-opus-4-1[1m]"}}"#;
    let file = inspect(AppKind::Claude, "s", Some(current));
    let proposal = import_proposal(&file, Some(current), None).expect("must be importable");

    assert_eq!(proposal.draft.model.as_deref(), Some("claude-opus-4-1"));
    let Some(ModelOptions::Claude(settings)) = proposal.draft.model_options.as_ref() else {
        panic!("Claude model settings should be imported");
    };
    assert!(settings.primary_one_m);
    assert_eq!(settings.sonnet_model.as_deref(), Some("claude-sonnet-4-6"));
    assert!(settings.sonnet_one_m);
    assert_eq!(settings.opus_model.as_deref(), Some("claude-opus-4-1"));
    assert!(settings.opus_one_m);
    assert!(proposal.draft.validate().is_ok());
}

#[test]
fn importing_claude_configuration_rejects_uppercase_one_m_before_import() {
    let current = r#"{"env":{"ANTHROPIC_BASE_URL":"https://relay.internal","ANTHROPIC_AUTH_TOKEN":"TEST_CLAUDE_IMPORT_KEY","ANTHROPIC_MODEL":"claude-opus-4-1[1M]"}}"#;
    let file = inspect(AppKind::Claude, "s", Some(current));
    let DiscoveredState::Ok {
        warnings,
        importable,
        ..
    } = &file.state
    else {
        panic!("should parse");
    };

    assert!(!importable);
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("1M 标记无效")));
    assert!(import_proposal(&file, Some(current), None).is_none());
}

#[test]
fn plaintext_token_gets_a_warning() {
    let current = CLAUDE_JSON.replace(
        "\"ANTHROPIC_MODEL\": \"claude-sonnet-4\"",
        "\"ANTHROPIC_AUTH_TOKEN\": \"sk-live-0123456789abcdef\"",
    );
    let file = inspect(AppKind::Claude, "s", Some(&current));
    let DiscoveredState::Ok { warnings, .. } = file.state else {
        panic!("should parse");
    };
    assert!(warnings.iter().any(|w| w.contains("ANTHROPIC_AUTH_TOKEN")));
    // The warning text never contains the token itself.
    assert!(!warnings.iter().any(|w| w.contains("sk-live")));
}

#[test]
fn non_openai_codex_provider_is_not_imported() {
    let current = r#"
model_provider = "gateway"
experimental_bearer_token = "TEST_CODEX_IMPORT_KEY"
"#;
    let file = inspect(AppKind::Codex, "c", Some(current));
    let DiscoveredState::Ok {
        warnings,
        importable,
        ..
    } = &file.state
    else {
        panic!("should parse");
    };
    assert!(!importable);
    assert!(warnings
        .iter()
        .any(|w| w.contains("无法作为供应商档案导入")));
    assert!(import_proposal(&file, Some(current), None).is_none());
}

#[test]
fn codex_anthropic_configuration_is_not_imported_without_an_explicit_limit() {
    let current = r#"
model_provider = "relay"
model = "claude-sandbox"

[model_providers.relay]
base_url = "https://relay.example/v1"
wire_api = "anthropic"
"#;
    let auth = r#"{"auth_mode":"apikey","OPENAI_API_KEY":"TEST_CODEX_IMPORT_KEY"}"#;
    let file = inspect(AppKind::Codex, "c", Some(current));
    let DiscoveredState::Ok {
        warnings,
        importable,
        ..
    } = &file.state
    else {
        panic!("should parse");
    };
    assert!(!importable);
    assert!(warnings
        .iter()
        .any(|warning| warning.contains("最大输出 token")));
    assert!(import_proposal(&file, Some(current), Some(auth)).is_none());
}

#[test]
fn default_responses_protocol_and_codex_run_options_are_imported() {
    let current = CODEX_TOML.replace(
        "threads = 8",
        "threads = 8\nmodel_reasoning_effort = \"xhigh\"\nmodel_context_window = 272000",
    );
    let file = inspect(AppKind::Codex, "c", Some(&current));

    let DiscoveredState::Ok {
        route, importable, ..
    } = &file.state
    else {
        panic!("should parse");
    };
    assert!(*importable);
    assert_eq!(route.wire_api, None);
    let proposal =
        import_proposal(&file, Some(&current), Some(CODEX_AUTH_JSON)).expect("must be importable");
    let Some(ModelOptions::Codex(options)) = proposal.draft.model_options else {
        panic!("Codex run options should be retained");
    };
    // Effort, summary, and verbosity are general settings now; the
    // profile keeps only the context window.
    assert_eq!(options.context_window, Some(272000));
}

#[test]
fn codex_official_route_is_imported_without_reading_a_token() {
    let current = "model = \"gpt-5\"\nthreads = 8\n";
    let file = inspect(AppKind::Codex, "c", Some(&current));

    let DiscoveredState::Ok {
        warnings,
        importable,
        ..
    } = &file.state
    else {
        panic!("should parse");
    };
    assert!(*importable);
    assert!(warnings.is_empty());
    let proposal = import_proposal(&file, Some(&current), None).expect("official route imports");
    assert_eq!(proposal.draft.route_mode, RouteMode::Official);
    assert!(proposal.draft.api_key.is_empty());
    assert!(proposal.draft.base_url.is_none());
}

#[test]
fn read_errors_are_distinct_from_missing_files() {
    let report = discover(
        &DiscoveryPaths {
            codex: "c".into(),
            codex_auth: "a".into(),
            claude: "s".into(),
        },
        |path| {
            if path == "c" {
                Err("无法读取配置文件".into())
            } else {
                Ok(None)
            }
        },
    );

    assert!(matches!(
        report.codex.state,
        DiscoveredState::ReadError { .. }
    ));
    assert!(report.import_proposals.is_empty());
}

#[test]
fn claude_official_route_discards_custom_model_tiers_on_import() {
    let current = r#"{"env":{"ANTHROPIC_DEFAULT_HAIKU_MODEL":"claude-haiku-4"}}"#;
    let file = inspect(AppKind::Claude, "s", Some(current));
    let DiscoveredState::Ok { importable, .. } = &file.state else {
        panic!("should parse");
    };
    assert!(*importable);
    let proposal = import_proposal(&file, Some(current), None).expect("official route imports");
    assert_eq!(proposal.draft.route_mode, RouteMode::Official);
    assert!(proposal.draft.model.is_none());
}
