use super::*;
use crate::contracts::{ConfigValue, ModelOptions, RouteMode, SettingValue};
use crate::test_support::{CLAUDE_JSON, CODEX_TOML};

#[test]
fn missing_files_are_reported_not_guessed() {
    let report = discover(
        &DiscoveryPaths {
            codex: "~/.codex/config.toml".into(),
            claude: "~/.claude/settings.json".into(),
        },
        |_| Ok(None),
    );
    assert_eq!(report.codex.state, DiscoveredState::Missing);
    assert_eq!(report.claude.state, DiscoveredState::Missing);
    assert!(report.claude_import_proposals.is_empty());
}

#[test]
fn parse_errors_carry_line_and_scrubbed_message() {
    let report = discover(
        &DiscoveryPaths {
            codex: "c".into(),
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
            claude: "s".into(),
        },
        |p| {
            if p == "c" {
                Ok(Some(CODEX_TOML.to_string()))
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
    assert_eq!(route.provider_name.as_deref(), Some("openai"));
    assert_eq!(route.wire_api, None);
    assert!(!report.claude_import_proposals.is_empty());
    let claude_proposal = report
        .claude_import_proposals
        .first()
        .expect("claude proposal");
    assert_eq!(claude_proposal.draft.api_key, "TEST_CLAUDE_IMPORT_KEY");

    // Only the cache copy is redacted; the live report keeps its keys.
    let cached = report.cached_display();
    assert!(!format!("{:?}", cached.codex).contains("fixture-a"));
    assert_eq!(cached.claude, report.claude);
    for proposal in &cached.claude_import_proposals {
        assert_eq!(proposal.draft.api_key, "");
    }
    for proposal in &report.claude_import_proposals {
        assert!(!proposal.draft.api_key.is_empty());
    }
}

#[test]
fn importing_claude_configuration_decodes_lowercase_one_m_into_semantic_state() {
    let current = r#"{"env":{"ANTHROPIC_BASE_URL":"https://relay.internal","ANTHROPIC_AUTH_TOKEN":"TEST_CLAUDE_IMPORT_KEY","ANTHROPIC_MODEL":"claude-opus-4-1[1m]","ANTHROPIC_DEFAULT_SONNET_MODEL":"claude-sonnet-4-6[1m]","ANTHROPIC_DEFAULT_OPUS_MODEL":"claude-opus-4-1[1m]"}}"#;
    let file = inspect(AppKind::Claude, "s", Some(current));
    let proposal = claude_import_proposal(&file, Some(current)).expect("must be importable");

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
    assert!(claude_import_proposal(&file, Some(current)).is_none());
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
fn codex_configuration_requires_a_new_specialized_profile() {
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
    assert!(warnings.iter().any(|w| w.contains("新建专用档案")));
    assert!(claude_import_proposal(&file, Some(current)).is_none());
}

#[test]
fn read_errors_are_distinct_from_missing_files() {
    let report = discover(
        &DiscoveryPaths {
            codex: "c".into(),
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
    assert!(report.claude_import_proposals.is_empty());
}

#[test]
fn claude_official_route_discards_custom_model_tiers_on_import() {
    let current = r#"{"env":{"ANTHROPIC_DEFAULT_HAIKU_MODEL":"claude-haiku-4"}}"#;
    let file = inspect(AppKind::Claude, "s", Some(current));
    let DiscoveredState::Ok { importable, .. } = &file.state else {
        panic!("should parse");
    };
    assert!(*importable);
    let proposal = claude_import_proposal(&file, Some(current)).expect("official route imports");
    assert_eq!(proposal.draft.route_mode, RouteMode::Official);
    assert!(proposal.draft.model.is_none());
}

#[test]
fn claude_import_preserves_explicit_parameters_without_client_preferences() {
    let current =
        r#"{"effortLevel":"high","alwaysThinkingEnabled":false,"spinnerTipsEnabled":true}"#;
    let file = inspect(AppKind::Claude, "settings.json", Some(current));
    let proposal = claude_import_proposal(&file, Some(current)).unwrap();
    assert_eq!(
        proposal.draft.parameters.value("effortLevel"),
        Some(&SettingValue::Explicit {
            value: ConfigValue::Str("high".into())
        })
    );
    assert_eq!(
        proposal.draft.parameters.value("alwaysThinkingEnabled"),
        Some(&SettingValue::Explicit {
            value: ConfigValue::Bool(false)
        })
    );
    assert!(!proposal
        .draft
        .parameters
        .settings
        .contains_key("spinnerTipsEnabled"));
    assert_eq!(
        proposal.draft.parameters.value("autoCompactEnabled"),
        Some(&SettingValue::Automatic)
    );
}

#[test]
fn invalid_provider_parameters_stop_import_instead_of_becoming_automatic() {
    for (app, current, key) in [(
        AppKind::Claude,
        r#"{"alwaysThinkingEnabled":"false"}"#,
        "alwaysThinkingEnabled",
    )] {
        let file = inspect(app, "configuration", Some(current));
        let DiscoveredState::Ok {
            importable,
            warnings,
            ..
        } = &file.state
        else {
            panic!("valid syntax")
        };
        assert!(!importable);
        assert!(warnings.iter().any(|warning| warning.contains(key)));
        assert!(claude_import_proposal(&file, Some(current)).is_none());
    }
}
