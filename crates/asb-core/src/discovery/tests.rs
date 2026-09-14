use super::*;
use crate::contracts::{ConfigValue, ModelOptions, RouteMode, SettingValue};
use crate::test_support::{CLAUDE_JSON, CODEX_TOML};

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
    assert!(report.claude_import_proposals.is_empty());
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
fn codex_gateway_configuration_is_managed_and_not_reimported() {
    let current = r#"
model_provider = "openai"
openai_base_url = "http://127.0.0.1:47821/codex/asb_codex_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef/v1"
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
    assert!(warnings.is_empty());
    assert!(claude_import_proposal(&file, Some(current)).is_none());
}

#[test]
fn codex_custom_provider_is_importable_and_reports_live_route_facts() {
    let current = r#"
model_provider = "relay"
model = "vendor-codex"

[model_providers.relay]
name = "Relay"
base_url = "https://relay.example/v1"
wire_api = "chat"
experimental_bearer_token = "TEST_CODEX_IMPORT_KEY"
"#;
    let file = inspect(AppKind::Codex, "c", Some(current));
    let DiscoveredState::Ok {
        route,
        importable,
        warnings,
        ..
    } = &file.state
    else {
        panic!("should parse");
    };
    assert!(*importable);
    assert!(warnings.is_empty());
    assert_eq!(route.provider_name.as_deref(), Some("Relay"));
    assert_eq!(route.base_url.as_deref(), Some("https://relay.example/v1"));
    assert_eq!(route.wire_api.as_deref(), Some("chat"));
}

#[test]
fn codex_live_import_reads_catalog_facts_and_protocol_authentication() {
    let current = r#"
model_provider = "relay"
model = "vendor-codex"
model_catalog_json = "models.json"

[model_providers.relay]
name = "Relay Display Name"
base_url = "https://relay.example"
wire_api = "anthropic"
"#;
    let catalog = r#"{
        "models": [{
            "slug": "vendor-codex",
            "display_name": "Vendor Codex",
            "description": "A catalog description",
            "base_instructions": "Use the vendor system prompt.",
            "supports_parallel_tool_calls": true,
            "context_window": 256000,
            "max_output_tokens": 8192,
            "input_modalities": ["text", "image"],
            "supported_reasoning_levels": ["low", "high"],
            "default_reasoning_level": "high"
        }]
    }"#;
    let source = crate::discovery::codex_import_source(
        "C:/codex/config.toml",
        current,
        Some(r#"{"OPENAI_API_KEY":"live-api-key"}"#),
        Some(catalog),
    )
    .expect("live Codex route should be importable");
    let crate::discovery::CodexImportAction::ThirdParty(draft) = source.action else {
        panic!("expected third-party draft");
    };
    assert_eq!(draft.name, "Relay Display Name");
    assert_eq!(draft.api_key, "live-api-key");
    assert_eq!(
        draft.authentication,
        Some(crate::contracts::AuthenticationScheme::XApiKey)
    );
    assert_eq!(
        draft.catalog[0].display_name.as_deref(),
        Some("Vendor Codex")
    );
    assert_eq!(
        draft.catalog[0].base_instructions.as_deref(),
        Some("Use the vendor system prompt.")
    );
    assert_eq!(draft.catalog[0].supports_parallel_tool_calls, Some(true));
    assert_eq!(draft.catalog[0].images, true);
}

#[test]
fn codex_live_import_warns_on_oauth_only_auth_without_importing_it() {
    let current = r#"
model_provider = "relay"
model = "vendor-codex"

[model_providers.relay]
name = "Relay"
base_url = "https://relay.example/v1"
wire_api = "responses"
"#;
    let source = crate::discovery::codex_import_source(
        "C:/codex/config.toml",
        current,
        Some(r#"{"tokens":{"access_token":"oauth-secret"}}"#),
        None,
    )
    .expect("OAuth-only route remains inspectable");
    let debug = format!("{source:?}");
    let crate::discovery::CodexImportAction::ThirdParty(ref draft) = source.action else {
        panic!("expected third-party draft");
    };
    assert!(draft.api_key.is_empty());
    assert_eq!(
        draft.authentication,
        Some(crate::contracts::AuthenticationScheme::Bearer)
    );
    assert!(source
        .proposal
        .warnings
        .iter()
        .any(|warning| warning.contains("OAuth")));
    assert!(!debug.contains("oauth-secret"));
}

#[test]
fn codex_custom_provider_diagnostics_are_added_to_discovery() {
    let report = discover(
        &DiscoveryPaths {
            codex: "c".into(),
            codex_auth: "a".into(),
            claude: "s".into(),
        },
        |path| {
            match path {
            "c" => Ok(Some(
                "model_provider = \"relay\"\n[model_providers.relay]\nbase_url = \"https://relay.example\"\nwire_api = \"responses\"\n".into(),
            )),
            _ => Ok(None),
        }
        },
    );
    let DiscoveredState::Ok {
        importable,
        warnings,
        ..
    } = report.codex.state
    else {
        panic!("should parse");
    };
    assert!(!importable);
    assert!(warnings.iter().any(|warning| warning.contains("主模型")));
    assert!(report.codex_import_proposals.is_empty());
}

#[test]
fn official_codex_proposal_does_not_expose_custom_model() {
    let report = discover(
        &DiscoveryPaths {
            codex: "c".into(),
            codex_auth: "a".into(),
            claude: "s".into(),
        },
        |path| match path {
            "c" => Ok(Some(
                "model_provider = \"openai\"\nmodel = \"gpt-custom\"\n".into(),
            )),
            _ => Ok(None),
        },
    );
    let proposal = report.codex_import_proposals.first().expect("proposal");
    assert!(proposal.official);
    assert!(proposal.model.is_none());
    assert!(proposal.upstream.is_none());
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
