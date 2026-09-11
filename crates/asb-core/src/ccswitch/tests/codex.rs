use super::*;

use crate::contracts::{CodexReasoningLevel, CodexUpstream, ConfigValue, SettingValue};
use serde_json::{json, Value};

const API_KEY: &str = "test-codex-key";

fn codex_seed(outcome: &CcSwitchProposal) -> &CodexImportSeed {
    match &outcome.draft {
        CcSwitchProviderDraft::Codex(seed) => seed,
        CcSwitchProviderDraft::Claude(_) => panic!("expected a Codex import seed"),
    }
}

/// The standard third-party template the source itself generates: a custom
/// provider table with the credential in `auth`.
fn third_party_config(base_url: &str, wire_api: &str) -> String {
    format!(
        r#"model_provider = "custom"
model = "gpt-5.6-sol"
model_reasoning_effort = "high"
disable_response_storage = true

[model_providers.custom]
name = "Relay"
base_url = "{base_url}"
wire_api = "{wire_api}"
requires_openai_auth = true
"#
    )
}

fn settings(config: &str, auth: Value, catalog: Option<Value>) -> String {
    let mut object = json!({ "auth": auth, "config": config });
    if let Some(catalog) = catalog {
        object["modelCatalog"] = catalog;
    }
    object.to_string()
}

fn source_row(
    config: &str,
    auth: Value,
    catalog: Option<Value>,
    meta: Option<Value>,
) -> CcSwitchRow {
    let mut source = row(
        "codex",
        "codex-1",
        "Relay Codex",
        &settings(config, auth, catalog),
    );
    source.meta = meta.map(|meta| meta.to_string());
    source
}

/// The catalog shape the source actually writes when the user fills its model
/// mapping table.
fn real_catalog() -> Value {
    json!({ "models": [
        {
            "model": "gpt-5.6-sol",
            "displayName": "GPT-5.6",
            "contextWindow": 272000,
            "supportsParallelToolCalls": true,
            "inputModalities": ["text", "image"],
            "baseInstructions": "You are a helpful assistant.",
            "reasoningLevels": ["low", "medium", "high"],
            "defaultReasoningLevel": "high"
        },
        {
            "model": "gpt-5.6-sol-mini",
            "contextWindow": "200000"
        }
    ]})
}

#[test]
fn codex_plain_row_without_a_catalog_yields_a_seed() {
    let source = source_row(
        &third_party_config("https://relay.example", "responses"),
        json!({ "OPENAI_API_KEY": API_KEY }),
        None,
        Some(json!({
            "apiFormat": "openai_responses",
            "costMultiplier": 0.8,
            "endpointAutoSelect": true
        })),
    );

    let outcome = map_row(&source).expect("a plain CC Switch row must map to a seed");
    let seed = codex_seed(&outcome);
    assert_eq!(seed.name, "Relay Codex");
    assert_eq!(seed.endpoint.0, "https://relay.example/v1");
    assert_eq!(seed.api_key, API_KEY);
    assert_eq!(seed.upstream, CodexUpstream::Responses);
    assert_eq!(seed.default_model, "gpt-5.6-sol");
    assert_eq!(seed.catalog.len(), 1);
    assert_eq!(seed.catalog[0].model, "gpt-5.6-sol");
    assert_eq!(seed.catalog[0].context_window, None);
    assert!(seed
        .warnings
        .contains(&"未导入: meta.costMultiplier".to_string()));
    assert!(seed
        .warnings
        .contains(&"未导入: meta.endpointAutoSelect".to_string()));
    assert_eq!(seed.warnings, outcome.warnings);
}

#[test]
fn codex_real_catalog_facts_seed_the_editor_catalog() {
    let source = source_row(
        &third_party_config("https://relay.example", "chat"),
        json!({ "OPENAI_API_KEY": API_KEY }),
        Some(real_catalog()),
        Some(json!({ "apiFormat": "openai_chat" })),
    );

    let outcome = map_row(&source).expect("a row with a real catalog must map to a seed");
    let seed = codex_seed(&outcome);
    assert_eq!(seed.upstream, CodexUpstream::ChatCompletions);
    assert_eq!(seed.catalog.len(), 2);
    let main = &seed.catalog[0];
    assert_eq!(main.model, "gpt-5.6-sol");
    assert_eq!(main.context_window, Some(272_000));
    assert_eq!(main.images, Some(true));
    assert_eq!(
        main.default_reasoning_level,
        Some(CodexReasoningLevel::High)
    );
    assert_eq!(
        main.reasoning_levels,
        Some(vec![
            CodexReasoningLevel::Low,
            CodexReasoningLevel::Medium,
            CodexReasoningLevel::High
        ])
    );
    let mini = &seed.catalog[1];
    assert_eq!(mini.model, "gpt-5.6-sol-mini");
    assert_eq!(mini.context_window, Some(200_000));
    assert_eq!(mini.images, None);
    for field in [
        "displayName",
        "supportsParallelToolCalls",
        "baseInstructions",
    ] {
        assert!(
            seed.warnings
                .iter()
                .any(|warning| warning.contains(&format!("modelCatalog.models[0].{field}"))),
            "missing warning for {field}"
        );
    }
}

#[test]
fn codex_catalog_appends_the_default_model_when_absent() {
    let catalog = json!({ "models": [
        { "model": "gpt-5.6-sol-mini", "contextWindow": 200000 }
    ]});
    let source = source_row(
        &third_party_config("https://relay.example", "responses"),
        json!({ "OPENAI_API_KEY": API_KEY }),
        Some(catalog),
        None,
    );

    let outcome = map_row(&source).expect("row should map");
    let seed = codex_seed(&outcome);
    assert_eq!(seed.catalog.len(), 2);
    assert_eq!(seed.catalog[1].model, "gpt-5.6-sol");
    assert_eq!(seed.catalog[1].context_window, None);
}

#[test]
fn codex_catalog_invalid_or_duplicate_facts_downgrade_to_warnings() {
    let catalog = json!({ "models": [
        { "model": "valid-model", "contextWindow": 128000 },
        { "model": "bad-context", "contextWindow": "not-a-number" },
        { "model": "bad-modality", "inputModalities": ["text", "video"] },
        { "model": "bad-levels", "reasoningLevels": ["low", "bogus"] },
        { "model": "mismatched-default", "reasoningLevels": ["low"], "defaultReasoningLevel": "high" },
        { "model": "valid-model" },
        "not-an-object"
    ]});
    let source = source_row(
        &third_party_config("https://relay.example", "responses"),
        json!({ "OPENAI_API_KEY": API_KEY }),
        Some(catalog),
        None,
    );

    let outcome = map_row(&source).expect("row should map");
    let seed = codex_seed(&outcome);
    let models: Vec<&str> = seed
        .catalog
        .iter()
        .map(|seed| seed.model.as_str())
        .collect();
    assert_eq!(
        models,
        vec![
            "valid-model",
            "bad-context",
            "bad-modality",
            "bad-levels",
            "mismatched-default",
            "gpt-5.6-sol"
        ]
    );
    for fragment in [
        "models[1].contextWindow 必须是正整数",
        "models[2].inputModalities 仅支持 text 或 image",
        "models[3].reasoningLevels 无法识别",
        "models[4].defaultReasoningLevel 不在 reasoningLevels 内",
        "models[5]（valid-model 与先前条目重复）",
        "models[6]",
    ] {
        assert!(
            seed.warnings
                .iter()
                .any(|warning| warning.contains(fragment)),
            "missing warning containing {fragment}"
        );
    }
    let mismatched = &seed.catalog[4];
    assert_eq!(mismatched.default_reasoning_level, None);
    assert_eq!(
        mismatched.reasoning_levels,
        Some(vec![CodexReasoningLevel::Low])
    );
}

#[test]
fn codex_real_reasoning_efforts_seed_parameters() {
    // Real source rows carry the efforts current Codex models expose (the
    // user's Any row uses "ultra", AIHub uses "max"; "none" and "minimal" are
    // the documented low end). The choice spec must accept the whole real
    // domain instead of rejecting the row.
    for effort in ["none", "minimal", "max", "ultra"] {
        let config = third_party_config("https://relay.example", "responses").replace(
            "model_reasoning_effort = \"high\"",
            &format!("model_reasoning_effort = \"{effort}\""),
        );
        let source = source_row(&config, json!({ "OPENAI_API_KEY": API_KEY }), None, None);

        let outcome =
            map_row(&source).expect("a real Codex reasoning effort must not reject the row");
        let seed = codex_seed(&outcome);
        assert_eq!(
            seed.parameters.settings.get("model_reasoning_effort"),
            Some(&SettingValue::Explicit {
                value: ConfigValue::Str(effort.to_string())
            }),
            "effort {effort} must survive into the seed parameters"
        );
    }
}

#[test]
fn codex_catalog_accepts_the_new_codex_reasoning_levels() {
    let catalog = json!({ "models": [
        {
            "model": "gpt-5.6-terra",
            "reasoningLevels": ["minimal", "low", "medium", "high", "xhigh", "max", "ultra"],
            "defaultReasoningLevel": "max"
        }
    ]});
    let source = source_row(
        &third_party_config("https://relay.example", "responses"),
        json!({ "OPENAI_API_KEY": API_KEY }),
        Some(catalog),
        None,
    );

    let outcome = map_row(&source).expect("new Codex levels must parse");
    let seed = codex_seed(&outcome);
    assert_eq!(
        seed.catalog[0].reasoning_levels,
        Some(vec![
            CodexReasoningLevel::Minimal,
            CodexReasoningLevel::Low,
            CodexReasoningLevel::Medium,
            CodexReasoningLevel::High,
            CodexReasoningLevel::Xhigh,
            CodexReasoningLevel::Max,
            CodexReasoningLevel::Ultra,
        ])
    );
    assert_eq!(
        seed.catalog[0].default_reasoning_level,
        Some(CodexReasoningLevel::Max)
    );
}

#[test]
fn codex_missing_credential_seeds_an_empty_key_with_a_warning() {
    let source = source_row(
        &third_party_config("https://relay.example", "responses"),
        json!({ "tokens": { "access_token": "oauth-source-token" } }),
        None,
        None,
    );

    let outcome = map_row(&source).expect("a row without a key still maps to a seed");
    let seed = codex_seed(&outcome);
    assert!(seed.api_key.is_empty());
    assert!(seed
        .warnings
        .iter()
        .any(|warning| warning.contains("API 密钥")));
    assert!(seed.warnings.contains(&"未导入: auth.tokens".to_string()));
    let debug = format!("{outcome:?}");
    assert!(!debug.contains("oauth-source-token"));
}

#[test]
fn codex_official_row_still_skips() {
    let source = source_row(
        "",
        json!({ "tokens": { "access_token": "oauth" } }),
        None,
        None,
    );

    let skipped = map_row(&source).expect_err("official rows are not importable");
    assert!(skipped.reason.contains("官方登录"));
}

#[test]
fn codex_unsupported_wire_api_or_api_format_skips() {
    let source = source_row(
        &third_party_config("https://relay.example", "grpc"),
        json!({ "OPENAI_API_KEY": API_KEY }),
        None,
        None,
    );
    let skipped = map_row(&source).expect_err("unsupported wire_api must skip");
    assert!(skipped.reason.contains("wire_api"));

    let source = source_row(
        &third_party_config("https://relay.example", "responses"),
        json!({ "OPENAI_API_KEY": API_KEY }),
        None,
        Some(json!({ "apiFormat": "weird" })),
    );
    let skipped = map_row(&source).expect_err("unsupported apiFormat must skip");
    assert!(skipped.reason.contains("apiFormat"));
}

#[test]
fn codex_unknown_settings_and_table_fields_become_warnings() {
    let config = format!(
        "{}env_key = \"RELAY_API_KEY\"\n",
        third_party_config("https://relay.example", "responses")
    );
    let mut source = source_row(&config, json!({ "OPENAI_API_KEY": API_KEY }), None, None);
    let mut settings: Value =
        serde_json::from_str(&source.settings_config).expect("fixture must parse");
    settings["extraFlag"] = json!(true);
    source.settings_config = settings.to_string();

    let outcome = map_row(&source).expect("env_key rows must map to a seed");
    let seed = codex_seed(&outcome);
    assert!(seed
        .warnings
        .contains(&"未导入: settings_config.extraFlag".to_string()));
    assert!(seed
        .warnings
        .contains(&"未导入: model_providers.custom.env_key".to_string()));
}

#[test]
fn codex_selected_inline_table_is_supported() {
    let config = r#"
model_provider = "chosen"
model = "codex-main"

[model_providers]
chosen = { base_url = "https://relay.example", wire_api = "chat", experimental_bearer_token = "test-codex-key" }
"#
    .to_string();
    let source = source_row(&config, json!({}), None, None);

    let outcome = map_row(&source).expect("inline selected provider should map");
    let seed = codex_seed(&outcome);
    assert_eq!(seed.endpoint.0, "https://relay.example/v1");
    assert_eq!(seed.upstream, CodexUpstream::ChatCompletions);
    assert_eq!(seed.api_key, API_KEY);
}

#[test]
fn codex_meta_api_format_overrides_the_wire_api() {
    let source = source_row(
        &third_party_config("https://relay.example", "chat"),
        json!({ "OPENAI_API_KEY": API_KEY }),
        None,
        Some(json!({ "apiFormat": "openai_responses" })),
    );

    let outcome = map_row(&source).expect("apiFormat must win");
    let seed = codex_seed(&outcome);
    assert_eq!(seed.upstream, CodexUpstream::Responses);
}

#[test]
fn codex_openai_root_uses_its_explicit_auth_key() {
    let config = r#"
model = "codex-main"
openai_base_url = "https://relay.example"
"#
    .to_string();
    let source = source_row(&config, json!({ "OPENAI_API_KEY": API_KEY }), None, None);

    let outcome = map_row(&source).expect("OpenAI root with an explicit key should map");
    let seed = codex_seed(&outcome);
    assert_eq!(seed.endpoint.0, "https://relay.example/v1");
    assert_eq!(seed.api_key, API_KEY);
    assert!(!outcome
        .warnings
        .iter()
        .any(|warning| warning == "未导入: auth.OPENAI_API_KEY"));
}

#[test]
fn codex_endpoint_normalization_preserves_an_explicit_prefix() {
    let config = r#"
model_provider = "custom"
model = "gpt-5.6-sol"

[model_providers.custom]
base_url = "https://relay.example/tenant/v2/"
wire_api = "responses"
experimental_bearer_token = "test-codex-key"
"#
    .to_string();
    let source = source_row(&config, json!({}), None, None);

    let outcome = map_row(&source).expect("prefixed endpoint should map");
    let seed = codex_seed(&outcome);
    assert_eq!(seed.endpoint.0, "https://relay.example/tenant/v2");
}

#[test]
fn codex_missing_main_model_skips() {
    let config = r#"
model_provider = "custom"

[model_providers.custom]
base_url = "https://relay.example"
wire_api = "responses"
experimental_bearer_token = "test-codex-key"
"#
    .to_string();
    let source = source_row(&config, json!({}), None, None);

    let skipped = map_row(&source).expect_err("the main model is required");
    assert!(skipped.reason.contains("主模型"));
}

#[test]
fn codex_seed_debug_never_exposes_the_credential() {
    let source = source_row(
        &third_party_config("https://relay.example", "responses"),
        json!({ "OPENAI_API_KEY": API_KEY }),
        None,
        None,
    );

    let outcome = map_row(&source).expect("row should map");
    let debug = format!("{outcome:?}");
    assert!(!debug.contains(API_KEY));
}
