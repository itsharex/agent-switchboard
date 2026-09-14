use super::*;

const CACHE_PATHS: [&str; 8] = [
    "/system/0/cache_control",
    "/messages/0/content/0/cache_control",
    "/messages/0/content/1/cache_control",
    "/messages/1/content/0/cache_control",
    "/messages/2/content/0/cache_control",
    "/messages/2/content/0/content/0/cache_control",
    "/messages/2/content/0/content/1/cache_control",
    "/tools/0/cache_control",
];

fn cached_request() -> Value {
    let mut input = request("gpt-5.4");
    input["system"] = json!([{ "type": "text", "text": "system", "cache_control": null }]);
    input["messages"] = json!([
        { "role": "user", "content": [
            { "type": "text", "text": "question", "cache_control": null },
            { "type": "image", "source": { "type": "url", "url": "https://example.com/a.png" },
              "cache_control": null },
        ] },
        { "role": "assistant", "content": [
            { "type": "tool_use", "id": "capture-1", "name": "capture", "input": {},
              "cache_control": null },
        ] },
        { "role": "user", "content": [
            { "type": "tool_result", "tool_use_id": "capture-1", "is_error": true,
              "cache_control": null, "content": [
                { "type": "text", "text": "partial capture", "cache_control": null },
                { "type": "image", "source": { "type": "base64", "media_type": "image/png",
                  "data": "CACHED_IMAGE" }, "cache_control": null },
              ] },
        ] },
    ]);
    input["tools"] = json!([{
        "name": "capture", "description": "Capture a window",
        "input_schema": { "type": "object", "properties": {} }, "cache_control": null,
    }]);
    for path in CACHE_PATHS {
        *input.pointer_mut(path).expect("cache node") = json!({ "type": "ephemeral" });
    }
    input
}

#[test]
fn ephemeral_cache_markers_are_validated_and_removed_at_every_node_for_both_targets() {
    for marker in [
        json!({ "type": "ephemeral" }),
        json!({ "type": "ephemeral", "ttl": "5m" }),
        json!({ "type": "ephemeral", "ttl": "1h" }),
    ] {
        let mut input = cached_request();
        for path in CACHE_PATHS {
            *input.pointer_mut(path).unwrap() = marker.clone();
        }
        for target in TARGETS {
            let value = convert(&input, target).expect("all legal cache nodes convert");
            let body = serde_json::to_string(&value).unwrap();
            assert!(!body.contains("cache_control"));
            assert!(body.contains("https://example.com/a.png"));
            assert!(body.contains("data:image/png;base64,CACHED_IMAGE"));
            assert!(body.contains("partial capture"));
            assert!(body.contains(TOOL_RESULT_ERROR_MARKER));
            assert_eq!(value["tools"].as_array().unwrap().len(), 1);
        }
    }
}

#[test]
fn invalid_cache_marker_shapes_are_rejected_consistently_at_every_node() {
    for marker in [
        Value::Null,
        json!("ephemeral"),
        json!({}),
        json!({ "type": "persistent" }),
        json!({ "type": "ephemeral", "ttl": "10m" }),
        json!({ "type": "ephemeral", "ttl": 300 }),
        json!({ "type": "ephemeral", "ttl": null }),
        json!({ "type": "ephemeral", "unknown": true }),
    ] {
        for path in CACHE_PATHS {
            let mut input = cached_request();
            *input.pointer_mut(path).unwrap() = marker.clone();
            for target in TARGETS {
                let error = convert(&input, target).expect_err("invalid cache marker");
                assert!(error.0.contains("cache_control"), "{path}: {}", error.0);
            }
        }
    }
}

#[test]
fn user_owned_cache_control_keys_in_tool_input_and_schema_are_not_stripped() {
    let mut input = cached_request();
    input["messages"][1]["content"][0]["input"] = json!({ "cache_control": "user data" });
    input["tools"][0]["input_schema"]["properties"] =
        json!({ "cache_control": { "type": "string" } });
    for target in TARGETS {
        let output = convert(&input, target).unwrap();
        let (schema, arguments) = match target {
            UpstreamProtocol::ChatCompletions => (
                &output["tools"][0]["function"]["parameters"],
                &output["messages"][2]["tool_calls"][0]["function"]["arguments"],
            ),
            UpstreamProtocol::Responses => (
                &output["tools"][0]["parameters"],
                &output["input"][1]["arguments"],
            ),
            _ => unreachable!(),
        };
        assert_eq!(schema["properties"]["cache_control"]["type"], "string");
        let arguments: Value = serde_json::from_str(arguments.as_str().unwrap()).unwrap();
        assert_eq!(arguments["cache_control"], "user data");
    }
}

#[test]
fn remote_image_sources_are_supported_without_fetching_and_bad_sources_are_rejected() {
    for target in TARGETS {
        for url in ["http://example.com/a.png", "https://example.com/b.png"] {
            let mut input = request("gpt-5.4");
            input["messages"][0]["content"] = json!([{
                "type": "image", "source": { "type": "url", "url": url },
                "cache_control": { "type": "ephemeral" },
            }]);
            assert!(serde_json::to_string(&convert(&input, target).unwrap())
                .unwrap()
                .contains(url));
            input["messages"][0]["content"][0]["source"]["url"] =
                json!("file:///private/image.png");
            assert!(convert(&input, target)
                .unwrap_err()
                .0
                .contains("image.source.url"));
        }
    }
}
