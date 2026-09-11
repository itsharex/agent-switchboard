use crate::gateway::transform::{convert_request, convert_response};
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Value};

#[test]
fn tool_search_loads_catalogue_and_preserves_the_client_result_for_chat() {
    let request = json!({
        "model": "sandbox-model",
        "input": [
            {
                "type": "tool_search_call",
                "id": "client-generated-search-item",
                "call_id": "call_search",
                "status": "completed",
                "execution": "client",
                "arguments": {"query": "mail", "limit": 3}
            },
            {
                "type": "tool_search_output",
                "call_id": "call_search",
                "tools": [{
                    "type": "namespace",
                    "name": "mcp__mail",
                    "tools": [{
                        "type": "function",
                        "name": "search",
                        "parameters": {"type": "object"}
                    }]
                }],
                "output": {"loaded": 1}
            },
            {
                "type": "message",
                "role": "user",
                "content": [{"type": "input_text", "text": "find unread mail"}]
            }
        ],
        "tools": [{
            "type": "tool_search", "execution": "client", "description": "Search tools",
            "parameters": {"type": "object", "properties": {"query": {"type": "string"}}}
        }]
    });

    let converted = convert_request(
        UpstreamProtocol::Responses,
        UpstreamProtocol::ChatCompletions,
        &serde_json::to_vec(&request).unwrap(),
        None,
        None,
        None,
    )
    .expect("tool search bridge must convert");
    let chat: Value = serde_json::from_slice(&converted.body).unwrap();

    let names = chat["tools"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|tool| tool.pointer("/function/name").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert!(names.contains(&"tool_search"));
    assert!(names.iter().any(|name| name.starts_with("asbns_n_")));
    assert_eq!(
        chat["messages"][0]["tool_calls"][0]["function"]["name"],
        "tool_search"
    );
    assert_eq!(chat["messages"][1]["role"], "tool");
    assert!(chat["messages"][1]["content"]
        .as_str()
        .is_some_and(|content| content.contains("mcp__mail")));
}

#[test]
fn rejects_invalid_client_generated_tool_search_item_id() {
    for id in [json!(""), json!(42)] {
        let request = json!({
            "model": "sandbox-model",
            "input": [{
                "type": "tool_search_call",
                "id": id,
                "call_id": "call_search",
                "status": "completed",
                "execution": "client",
                "arguments": {"query": "mail"}
            }]
        });
        let result = convert_request(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            &serde_json::to_vec(&request).unwrap(),
            None,
            None,
            None,
        );
        let Err(error) = result else {
            panic!("invalid client-generated item id must be rejected");
        };
        assert!(error.0.contains("tool_search_call.id"));
    }
}

#[test]
fn chat_tool_search_call_restores_codex_client_semantics() {
    let response = json!({
        "id": "chatcmpl_1",
        "object": "chat.completion",
        "model": "sandbox-model",
        "choices": [{
            "index": 0,
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": "call_search",
                    "type": "function",
                    "function": {
                        "name": "tool_search",
                        "arguments": "{\"query\":\"mail\",\"limit\":3}"
                    }
                }]
            },
            "finish_reason": "tool_calls"
        }]
    });

    let converted = convert_response(
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::Responses,
        &serde_json::to_vec(&response).unwrap(),
        None,
    )
    .expect("tool search call must restore");
    let responses: Value = serde_json::from_slice(&converted).unwrap();
    let call = &responses["output"][0];
    assert_eq!(call["type"], "tool_search_call");
    assert!(
        call.get("id").is_none(),
        "tool_search_call uses call_id as its only identifier"
    );
    assert_eq!(call["call_id"], "call_search");
    assert_eq!(call["execution"], "client");
    assert_eq!(call["arguments"]["query"], "mail");
}

#[test]
fn responses_tool_history_accepts_all_bridgeable_call_kinds_for_both_targets() {
    let request = json!({
        "model": "sandbox-model",
        "input": [
            {
                "type": "function_call",
                "id": "fc_item",
                "call_id": "call_function",
                "name": "lookup",
                "namespace": "mcp_mail",
                "arguments": "{\"query\":\"inbox\"}",
                "status": "completed"
            },
            {
                "type": "function_call_output",
                "id": "fco_item",
                "call_id": "call_function",
                "name": "lookup",
                "namespace": "mcp_mail",
                "output": {"matches": 2},
                "status": "completed"
            },
            {
                "type": "custom_tool_call",
                "id": "custom_item",
                "call_id": "call_custom",
                "name": "apply_patch",
                "input": "*** Begin Patch\n*** End Patch",
                "status": "completed"
            },
            {
                "type": "custom_tool_call_output",
                "id": "custom_output_item",
                "call_id": "call_custom",
                "output": [
                    {"type": "input_text", "text": "patched"}
                ],
                "status": "completed"
            },
            {
                "type": "tool_search_call",
                "id": "search_item",
                "call_id": "call_search",
                "status": "completed",
                "execution": "client",
                "arguments": {"query": "mail"}
            },
            {
                "type": "tool_search_output",
                "id": "search_output_item",
                "call_id": "call_search",
                "status": "completed",
                "execution": "client",
                "tools": [],
                "output": {"loaded": 0}
            }
        ],
        "tools": [
            {
                "type": "namespace",
                "name": "mcp_mail",
                "tools": [{
                    "type": "function",
                    "name": "lookup",
                    "parameters": {"type": "object"}
                }]
            },
            {"type": "custom", "name": "apply_patch"},
            {"type": "tool_search", "execution": "client"}
        ]
    });

    for target in [
        UpstreamProtocol::ChatCompletions,
        UpstreamProtocol::AnthropicMessages,
    ] {
        let converted = convert_request(
            UpstreamProtocol::Responses,
            target,
            &serde_json::to_vec(&request).unwrap(),
            Some(1024),
            None,
            None,
        )
        .expect("all Responses tool call kinds should bridge");
        let value: Value = serde_json::from_slice(&converted.body).unwrap();
        let body = serde_json::to_string(&value).unwrap();
        assert!(body.contains("call_function"));
        assert!(body.contains("matches"));
        assert!(body.contains("call_custom"));
        assert!(body.contains("custom_tool_call_output"));
        assert!(body.contains("call_search"));
        assert!(body.contains("tool_search_output"));
    }
}

#[test]
fn responses_function_output_serializes_unmapped_values_instead_of_dropping_them() {
    let outputs = [
        json!({"nested": {"answer": 42}}),
        json!([{"type": "unknown_content", "payload": "keep-me"}]),
    ];
    for output in outputs {
        let request = json!({
            "model": "sandbox-model",
            "input": [
                {
                    "type": "function_call",
                    "call_id": "call_value",
                    "name": "lookup",
                    "arguments": "{}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_value",
                    "output": output
                }
            ],
            "tools": [{
                "type": "function",
                "name": "lookup",
                "parameters": {"type": "object"}
            }]
        });
        let converted = convert_request(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            &serde_json::to_vec(&request).unwrap(),
            None,
            None,
            None,
        )
        .expect("unmapped tool output must remain visible as JSON");
        let chat: Value = serde_json::from_slice(&converted.body).unwrap();
        let content = chat["messages"][1]["content"].as_str().unwrap();
        assert!(content.contains("nested") || content.contains("unknown_content"));
        assert!(content.contains("keep-me") || content.contains("answer"));
    }
}

#[test]
fn responses_custom_and_tool_search_outputs_keep_string_object_and_content_array_shapes() {
    let output_shapes = [
        ("string", json!("plain-result")),
        ("object", json!({"value": 7})),
        (
            "content-array",
            json!([{"type": "input_text", "text": "array-result"}]),
        ),
    ];
    for (label, output) in output_shapes {
        let request = json!({
            "model": "sandbox-model",
            "input": [
                {
                    "type": "custom_tool_call",
                    "call_id": "call_custom",
                    "name": "apply_patch",
                    "input": "patch"
                },
                {
                    "type": "custom_tool_call_output",
                    "call_id": "call_custom",
                    "output": output,
                    "status": "completed"
                },
                {
                    "type": "tool_search_call",
                    "call_id": "call_search",
                    "status": "completed",
                    "execution": "client",
                    "arguments": {"query": "mail"}
                },
                {
                    "type": "tool_search_output",
                    "call_id": "call_search",
                    "tools": [],
                    "output": output
                }
            ],
            "tools": [
                {"type": "custom", "name": "apply_patch"},
                {"type": "tool_search", "execution": "client"}
            ]
        });
        for target in [
            UpstreamProtocol::ChatCompletions,
            UpstreamProtocol::AnthropicMessages,
        ] {
            let converted = convert_request(
                UpstreamProtocol::Responses,
                target,
                &serde_json::to_vec(&request).unwrap(),
                Some(1024),
                None,
                None,
            )
            .unwrap_or_else(|error| panic!("{label} output should bridge: {error}"));
            let body = String::from_utf8(converted.body).unwrap();
            assert!(body.contains("custom_tool_call_output"));
            assert!(body.contains("tool_search_output"));
            match label {
                "string" => assert!(body.contains("plain-result")),
                "object" => assert!(body.contains("value") && body.contains('7')),
                "content-array" => assert!(body.contains("array-result")),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn responses_tool_history_rejects_unpaired_or_mismatched_outputs() {
    let base = json!({
        "model": "sandbox-model",
        "input": [
            {
                "type": "function_call",
                "call_id": "call_function",
                "name": "lookup",
                "namespace": "mcp_mail",
                "arguments": "{}"
            }
        ],
        "tools": [{
            "type": "namespace",
            "name": "mcp_mail",
            "tools": [{"type": "function", "name": "lookup"}]
        }]
    });
    let cases = [
        (
            json!({
                "type": "function_call_output",
                "call_id": "missing",
                "output": "x"
            }),
            "没有可配对",
        ),
        (
            json!({
                "type": "function_call_output",
                "call_id": "call_function",
                "name": "other",
                "output": "x"
            }),
            "name",
        ),
        (
            json!({
                "type": "function_call_output",
                "call_id": "call_function",
                "namespace": "other",
                "output": "x"
            }),
            "namespace",
        ),
        (
            json!({
                "type": "custom_tool_call_output",
                "call_id": "call_function",
                "output": "x"
            }),
            "不能作为",
        ),
    ];
    for (output, expected) in cases {
        let mut request = base.clone();
        request["input"].as_array_mut().unwrap().push(output);
        let error = match convert_request(
            UpstreamProtocol::Responses,
            UpstreamProtocol::ChatCompletions,
            &serde_json::to_vec(&request).unwrap(),
            None,
            None,
            None,
        ) {
            Ok(_) => panic!("invalid tool history must fail"),
            Err(error) => error,
        };
        assert!(error.0.contains(expected), "{expected:?} in {:?}", error.0);
    }
}
