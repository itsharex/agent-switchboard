use super::*;
use std::io::Cursor;

struct Turn<'a> {
    tool: bool,
    args: Value,
    text: &'a str,
    step: Option<usize>,
}
impl Turn<'_> {
    fn id(&self, prefix: &str) -> String {
        format!(
            "{prefix}_cli_{}",
            self.step
                .map(|step| step.to_string())
                .unwrap_or_else(|| "probe".into())
        )
    }
}

pub(super) fn reasoning() -> Value {
    json!({"type":"reasoning","id":"rs_cli","summary":[],"encrypted_content":"fixture-native-reasoning"})
}

pub(super) fn reply(
    protocol: UpstreamProtocol,
    scenario: Scenario,
    step: Option<usize>,
    file: &Path,
    stream: bool,
) -> Response<Cursor<Vec<u8>>> {
    let tool = step.is_some_and(|step| {
        step < if matches!(scenario, Scenario::Recover) {
            2
        } else {
            1
        }
    });
    let file = if tool && step == Some(0) && matches!(scenario, Scenario::Recover) {
        file.with_file_name("missing.txt")
    } else {
        file.to_path_buf()
    };
    let turn = Turn {
        tool,
        args: json!({"file_path":file.to_string_lossy()}),
        text: if step.is_none() {
            "probe ok"
        } else {
            "claude parity ok"
        },
        step,
    };
    let body = if stream {
        match protocol {
            asb_core::UpstreamProtocol::GeminiGenerateContent => {
                panic!("Google native has a separate Claude fixture")
            }
            UpstreamProtocol::ChatCompletions => chat_stream(&turn),
            UpstreamProtocol::Responses => responses_stream(&turn),
            UpstreamProtocol::AnthropicMessages => anthropic_stream(&turn),
        }
        .into_bytes()
    } else {
        json_body(protocol, &turn).to_string().into_bytes()
    };
    Response::from_data(body).with_header(content_type(if stream {
        "text/event-stream"
    } else {
        "application/json"
    }))
}

fn json_body(protocol: UpstreamProtocol, turn: &Turn<'_>) -> Value {
    match protocol {
        asb_core::UpstreamProtocol::GeminiGenerateContent => {
            panic!("Google native has a separate Claude fixture")
        }
        UpstreamProtocol::ChatCompletions => {
            let message = if turn.tool {
                json!({"role":"assistant","content":null,"tool_calls":[{"id":turn.id("call"),"type":"function","function":{"name":"Read","arguments":turn.args.to_string()}}]})
            } else {
                json!({"role":"assistant","content":turn.text})
            };
            json!({"id":turn.id("chat"),"object":"chat.completion","model":"gpt-5.4","choices":[{"index":0,"message":message,"finish_reason":if turn.tool {"tool_calls"} else {"stop"}}],"usage":{"prompt_tokens":19,"completion_tokens":1,"total_tokens":20}})
        }
        UpstreamProtocol::Responses => responses_body(turn),
        UpstreamProtocol::AnthropicMessages => {
            let content = if turn.tool {
                json!([{"type":"tool_use","id":turn.id("call"),"name":"Read","input":turn.args}])
            } else {
                json!([{"type":"text","text":turn.text}])
            };
            json!({"id":turn.id("msg"),"type":"message","role":"assistant","model":"gpt-5.4","content":content,"stop_reason":if turn.tool {"tool_use"} else {"end_turn"},"stop_sequence":null,"usage":{"input_tokens":19,"output_tokens":1}})
        }
    }
}

fn chat_chunk(turn: &Turn<'_>, delta: Value, finish: Option<&str>) -> Value {
    json!({"id":turn.id("chat"),"model":"gpt-5.4","choices":[{"index":0,"delta":delta,"finish_reason":finish}]})
}

fn chat_stream(turn: &Turn<'_>) -> String {
    let chunks = if turn.tool {
        vec![
            chat_chunk(
                turn,
                json!({"role":"assistant","tool_calls":[{"index":0,"id":turn.id("call"),"type":"function","function":{"name":"Read","arguments":""}}]}),
                None,
            ),
            chat_chunk(
                turn,
                json!({"tool_calls":[{"index":0,"function":{"arguments":turn.args.to_string()}}]}),
                Some("tool_calls"),
            ),
        ]
    } else {
        vec![
            chat_chunk(turn, json!({"role":"assistant","content":turn.text}), None),
            chat_chunk(turn, json!({}), Some("stop")),
        ]
    };
    let mut output: String = chunks
        .into_iter()
        .map(|chunk| format!("data: {chunk}\n\n"))
        .collect();
    output.push_str(&format!("data: {}\n\ndata: [DONE]\n\n",json!({"id":turn.id("chat"),"model":"gpt-5.4","choices":[],"usage":{"prompt_tokens":19,"completion_tokens":1,"total_tokens":20}})));
    output
}

fn event(kind: &str, value: Value) -> String {
    format!("event: {kind}\ndata: {value}\n\n")
}

fn responses_body(turn: &Turn<'_>) -> Value {
    let output = if turn.tool {
        json!([reasoning(),{"type":"function_call","id":turn.id("fc"),"call_id":turn.id("call"),"name":"Read","arguments":turn.args.to_string(),"status":"completed"}])
    } else {
        json!([{"type":"message","id":turn.id("msg"),"role":"assistant","status":"completed","content":[{"type":"output_text","text":turn.text,"annotations":[]}]}])
    };
    json!({"id":turn.id("resp"),"object":"response","model":"gpt-5.4","status":"completed","output":output,"error":null,"usage":{"input_tokens":19,"output_tokens":1,"total_tokens":20}})
}

fn responses_stream(turn: &Turn<'_>) -> String {
    event(
        "response.completed",
        json!({"type":"response.completed","response":responses_body(turn)}),
    )
}

fn anthropic_stream(turn: &Turn<'_>) -> String {
    let block = if turn.tool {
        json!({"type":"tool_use","id":turn.id("call"),"name":"Read","input":{}})
    } else {
        json!({"type":"text","text":""})
    };
    let delta = if turn.tool {
        json!({"type":"input_json_delta","partial_json":turn.args.to_string()})
    } else {
        json!({"type":"text_delta","text":turn.text})
    };
    [
        event("message_start",json!({"type":"message_start","message":{"id":turn.id("msg"),"type":"message","role":"assistant","model":"gpt-5.4","content":[],"stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":19,"output_tokens":0}}})),
        event("content_block_start",json!({"type":"content_block_start","index":0,"content_block":block})),
        event("content_block_delta",json!({"type":"content_block_delta","index":0,"delta":delta})),
        event("content_block_stop",json!({"type":"content_block_stop","index":0})),
        event("message_delta",json!({"type":"message_delta","delta":{"stop_reason":if turn.tool {"tool_use"} else {"end_turn"},"stop_sequence":null},"usage":{"output_tokens":1}})),
        event("message_stop",json!({"type":"message_stop"})),
    ].concat()
}
