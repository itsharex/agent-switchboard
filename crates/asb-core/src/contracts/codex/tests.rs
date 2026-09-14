use super::*;

fn profile() -> CodexProviderProfile {
    CodexProviderProfile {
        id: "provider-1".to_string(),
        name: "Relay".to_string(),
        route_mode: CodexRouteMode::for_upstream(CodexUpstream::ChatCompletions),
        endpoint: CodexEndpoint("https://relay.example/v1".to_string()),
        api_key: "secret".to_string(),
        authentication: None,
        connection: Default::default(),
        upstream: CodexUpstream::ChatCompletions,
        request_mode: ResponsesRequestMode::Standard,
        default_model: "codex".to_string(),
        catalog: vec![CodexCatalogEntry {
            id: "codex".to_string(),
            context_window: 128_000,
            max_output_tokens: 16_384,
            function_tools: true,
            custom_tools: true,
            tool_search: true,
            reasoning: true,
            default_reasoning_level: CodexReasoningLevel::High,
            supported_reasoning_levels: vec![CodexReasoningLevel::None, CodexReasoningLevel::High],
            images: true,
            compact: true,
            display_name: None,
            description: None,
            base_instructions: None,
            supports_parallel_tool_calls: None,
        }],
        model_routes: vec![CodexModelRoute {
            client_model: "codex".to_string(),
            upstream_model: "vendor-codex".to_string(),
        }],
        capabilities: CodexCapabilities {
            responses: true,
            compact: true,
            models: true,
            chat_completions: true,
            alpha_search: false,
            image_generation: true,
            image_edit: true,
            function_tools: true,
            custom_tools: true,
            tool_search: true,
            reasoning: true,
            chat_reasoning: CodexChatReasoning::Configured {
                thinking_parameter: CodexChatThinkingParameter::None,
                effort_parameter: CodexChatEffortParameter::ReasoningEffort,
                effort_mode: CodexChatEffortMode::LowHigh,
            },
        },
    }
}

fn draft() -> CodexProviderDraft {
    let profile = profile();
    CodexProviderDraft {
        name: profile.name,
        endpoint: profile.endpoint,
        api_key: profile.api_key,
        authentication: profile.authentication,
        connection: profile.connection,
        upstream: profile.upstream,
        request_mode: profile.request_mode,
        default_model: profile.default_model,
        catalog: profile.catalog,
        model_routes: profile.model_routes,
        capabilities: profile.capabilities,
        parameters: crate::ownership::default_provider_parameters(super::super::AppKind::Codex),
        notes: None,
        website_url: None,
        usage_query: None,
    }
}

#[test]
fn profile_requires_a_complete_catalog_and_unique_client_routes() {
    let mut value = profile();
    assert!(value.validate().is_ok());
    value.model_routes.push(value.model_routes[0].clone());
    assert!(value.validate().unwrap_err().contains("重复客户端模型"));
}

#[test]
fn model_resolution_keeps_selected_catalog_models_across_protocols() {
    for protocol in [
        CodexUpstream::ChatCompletions,
        CodexUpstream::AnthropicMessages,
        CodexUpstream::Responses,
    ] {
        let mut value = profile();
        value.upstream = protocol;
        value.route_mode =
            CodexRouteMode::for_connection(protocol, &value.connection, value.authentication);
        if protocol != CodexUpstream::ChatCompletions {
            value.capabilities.chat_reasoning = CodexChatReasoning::Unsupported;
        }
        let mut second = value.catalog[0].clone();
        second.id = "second-model".into();
        value.catalog.push(second);
        let snapshot = CodexRouteSnapshot::from_profile(&value, "revision".into()).unwrap();
        assert_eq!(value.resolve_model("codex").unwrap(), "vendor-codex");
        assert_eq!(snapshot.resolve_model("codex").unwrap(), "vendor-codex");
        assert_eq!(value.resolve_model("second-model").unwrap(), "second-model");
        assert_eq!(
            snapshot.resolve_model("second-model").unwrap(),
            "second-model"
        );
        assert!(value.resolve_model("not-in-catalog").is_err());
        assert!(snapshot.resolve_model("not-in-catalog").is_err());
    }
}

#[test]
fn unknown_fields_are_rejected() {
    let mut value = serde_json::to_value(profile()).unwrap();
    value["unexpected"] = serde_json::Value::Bool(true);
    assert!(serde_json::from_value::<CodexProviderProfile>(value).is_err());
}

#[test]
fn chat_reasoning_uses_camel_case_field_names() {
    let value = serde_json::to_value(profile()).unwrap();
    let reasoning = &value["capabilities"]["chatReasoning"];
    assert_eq!(reasoning["kind"], "configured");
    assert_eq!(reasoning["thinkingParameter"], "none");
    assert_eq!(reasoning["effortParameter"], "reasoningEffort");
    assert_eq!(reasoning["effortMode"], "lowHigh");
    assert!(reasoning.get("thinking_parameter").is_none());
}

#[test]
fn snapshots_reject_models_outside_the_accepted_catalog() {
    let snapshot = CodexRouteSnapshot::from_profile(&profile(), "r1".to_string()).unwrap();
    assert_eq!(snapshot.resolve_model("codex").unwrap(), "vendor-codex");
    assert!(snapshot.resolve_model("not-in-catalog").is_err());
}

#[test]
fn openai_compatible_endpoints_preserve_a_vendor_root_without_inventing_v1() {
    let mut value = profile();
    value.endpoint = CodexEndpoint("https://relay.example".to_string());
    value.validate().unwrap();
    assert_eq!(
        crate::endpoint::upstream_endpoint(&value.endpoint.0, value.upstream.protocol()).unwrap(),
        "https://relay.example/chat/completions"
    );
}

#[test]
fn persisted_file_has_one_version_and_rejects_a_generic_shape() {
    let file = draft().into_file("00000000-0000-0000-0000-000000000001".to_string(), 100);
    file.validate().unwrap();
    let value = serde_json::to_value(&file).unwrap();
    assert!(serde_json::from_value::<CodexProviderFile>(value).is_ok());
    assert!(
        serde_json::from_value::<CodexProviderFile>(serde_json::json!({
            "id": "old", "baseUrl": "https://relay.example/v1"
        }))
        .is_err()
    );
}

#[test]
fn catalog_projection_includes_the_current_cli_required_fields() {
    let file = draft().into_file("00000000-0000-0000-0000-000000000001".to_string(), 100);
    let catalog: serde_json::Value =
        serde_json::from_str(&file.model_catalog_json().unwrap()).unwrap();
    let model = &catalog["models"][0];
    assert_eq!(model["slug"], "codex");
    assert_eq!(model["context_window"], 128_000);
    assert_eq!(
        model["input_modalities"],
        serde_json::json!(["text", "image"])
    );
    assert!(model["base_instructions"].is_string());
    assert!(model["supported_reasoning_levels"].is_array());
    assert!(model["supports_reasoning_summaries"].is_boolean());
    assert!(model["supports_parallel_tool_calls"].is_boolean());
    assert!(model["max_context_window"].is_number());
    assert_eq!(model["truncation_policy"]["mode"], "bytes");
    assert!(model["truncation_policy"]["limit"].is_number());
}

#[test]
fn catalog_projection_uses_the_profile_owned_reasoning_levels() {
    let mut file = draft().into_file("00000000-0000-0000-0000-000000000001".to_string(), 100);
    let entry = &mut file.profile.catalog[0];
    entry.default_reasoning_level = CodexReasoningLevel::Medium;
    entry.supported_reasoning_levels = vec![
        CodexReasoningLevel::None,
        CodexReasoningLevel::Low,
        CodexReasoningLevel::Medium,
    ];

    let catalog: serde_json::Value =
        serde_json::from_str(&file.model_catalog_json().unwrap()).unwrap();
    let model = &catalog["models"][0];
    assert_eq!(model["default_reasoning_level"], "medium");
    assert_eq!(
        model["supported_reasoning_levels"],
        serde_json::json!([
            {"effort": "none", "description": "Disable Thinking"},
            {"effort": "low", "description": "Low Thinking"},
            {"effort": "medium", "description": "Medium Thinking"},
        ])
    );
}

#[test]
fn catalog_rejects_incoherent_reasoning_declarations() {
    let mut file = draft().into_file("00000000-0000-0000-0000-000000000001".to_string(), 100);
    file.profile.catalog[0].supported_reasoning_levels = vec![];
    assert!(file.validate().unwrap_err().contains("未声明推理档位"));
}

#[test]
fn persisted_files_reject_incoherent_capability_declarations() {
    let id = "00000000-0000-0000-0000-000000000001".to_string();
    let mut without_responses = draft().into_file(id.clone(), 100);
    without_responses.profile.capabilities.responses = false;
    assert!(without_responses
        .validate()
        .unwrap_err()
        .contains("必须声明 responses"));

    let mut unsupported_tools = draft().into_file(id.clone(), 100);
    unsupported_tools.profile.capabilities.function_tools = false;
    assert!(unsupported_tools
        .validate()
        .unwrap_err()
        .contains("functionTools 超出供应商能力"));

    let mut non_chat_reasoning = draft().into_file(id, 100);
    non_chat_reasoning.profile.upstream = CodexUpstream::Responses;
    non_chat_reasoning.profile.route_mode = CodexRouteMode::for_connection(
        CodexUpstream::Responses,
        &non_chat_reasoning.profile.connection,
        non_chat_reasoning.profile.authentication,
    );
    assert!(non_chat_reasoning
        .validate()
        .unwrap_err()
        .contains("非 Chat Completions 上游"));
}
