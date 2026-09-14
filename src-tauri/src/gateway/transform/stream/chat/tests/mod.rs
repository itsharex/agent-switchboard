mod completion;
mod tools;
mod validation;

use super::super::{parse_frame, Frame, SseTranscoder, StreamTransformer};
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Value};
use std::io::Read;

const TARGETS: [UpstreamProtocol; 2] = [
    UpstreamProtocol::AnthropicMessages,
    UpstreamProtocol::Responses,
];

struct ChatStream {
    transformer: StreamTransformer,
    events: Vec<Value>,
}

impl ChatStream {
    fn new(target: UpstreamProtocol) -> Self {
        Self {
            transformer: StreamTransformer::new(UpstreamProtocol::ChatCompletions, target, None)
                .expect("supported route"),
            events: Vec::new(),
        }
    }

    fn push(&mut self, value: Value) {
        let bytes = self
            .transformer
            .on_frame(Frame {
                event: None,
                data: value.to_string(),
            })
            .expect("valid Chat frame");
        self.events.extend(decode_events(&bytes));
    }

    fn done(&mut self) {
        let bytes = self
            .transformer
            .on_frame(Frame {
                event: None,
                data: "[DONE]".to_string(),
            })
            .expect("valid terminal frame");
        self.events.extend(decode_events(&bytes));
        assert!(self
            .transformer
            .finish()
            .expect("complete stream")
            .is_empty());
    }
}

fn chunk(delta: Value) -> Value {
    json!({
        "id": "chat_fixture",
        "object": "chat.completion.chunk",
        "created": 0,
        "model": "test-model",
        "choices": [{ "index": 0, "delta": delta, "finish_reason": null }],
        "usage": null,
    })
}

fn finish(reason: &str) -> Value {
    let mut value = chunk(json!({}));
    value["choices"][0]["finish_reason"] = json!(reason);
    value
}

fn encode_frames(frames: &[Value], done: bool) -> String {
    let mut source = frames
        .iter()
        .map(|frame| format!("data: {frame}\n\n"))
        .collect::<String>();
    if done {
        source.push_str("data: [DONE]\n\n");
    }
    source
}

fn decode_events(bytes: &[u8]) -> Vec<Value> {
    std::str::from_utf8(bytes)
        .expect("UTF-8 events")
        .split("\n\n")
        .filter(|block| !block.is_empty())
        .map(|block| {
            let frame = parse_frame(block.as_bytes())
                .expect("valid SSE")
                .expect("data event");
            let value: Value = serde_json::from_str(&frame.data).expect("valid event JSON");
            assert_eq!(frame.event.as_deref(), value["type"].as_str());
            value
        })
        .collect()
}

fn events_of<'a>(events: &'a [Value], kind: &str) -> Vec<&'a Value> {
    events
        .iter()
        .filter(|event| event["type"] == kind)
        .collect()
}

fn call_start<'a>(events: &'a [Value], target: UpstreamProtocol, id: &str) -> (u64, &'a Value) {
    let (kind, item_field, id_field, index_field) = match target {
        asb_core::UpstreamProtocol::GeminiGenerateContent => panic!("Google native has a separate Claude fixture"),
        UpstreamProtocol::AnthropicMessages => {
            ("content_block_start", "content_block", "id", "index")
        }
        UpstreamProtocol::Responses => (
            "response.output_item.added",
            "item",
            "call_id",
            "output_index",
        ),
        UpstreamProtocol::ChatCompletions => unreachable!(),
    };
    let starts = events_of(events, kind)
        .into_iter()
        .filter(|event| event[item_field][id_field] == id)
        .collect::<Vec<_>>();
    assert_eq!(starts.len(), 1, "one start for {id}");
    (
        starts[0][index_field].as_u64().expect("output index"),
        &starts[0][item_field],
    )
}

fn argument_deltas(events: &[Value], target: UpstreamProtocol, index: u64) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match target {
            UpstreamProtocol::AnthropicMessages
                if event["type"] == "content_block_delta" && event["index"] == index =>
            {
                event["delta"]["partial_json"].as_str()
            }
            UpstreamProtocol::Responses
                if event["type"] == "response.function_call_arguments.delta"
                    && event["output_index"] == index =>
            {
                event["delta"].as_str()
            }
            _ => None,
        })
        .collect()
}

fn assert_function_call(
    events: &[Value],
    target: UpstreamProtocol,
    id: &str,
    name: &str,
    args: &str,
) {
    let (index, start) = call_start(events, target, id);
    assert_eq!(start["name"], name);
    assert_eq!(argument_deltas(events, target, index).concat(), args);
    match target {
        asb_core::UpstreamProtocol::GeminiGenerateContent => panic!("Google native has a separate Claude fixture"),
        UpstreamProtocol::AnthropicMessages => {
            assert_eq!(start["type"], "tool_use");
            assert_eq!(start["input"], json!({}));
            assert_eq!(
                events_of(events, "content_block_stop")
                    .iter()
                    .filter(|event| event["index"] == index)
                    .count(),
                1
            );
        }
        UpstreamProtocol::Responses => {
            assert_eq!(start["type"], "function_call");
            assert_eq!(start["arguments"], "");
            let done = events_of(events, "response.output_item.done")
                .into_iter()
                .filter(|event| event["output_index"] == index)
                .collect::<Vec<_>>();
            assert_eq!(done.len(), 1);
            assert_eq!(done[0]["item"]["arguments"], args);
            assert_eq!(done[0]["item"]["call_id"], id);
            let completed = events_of(events, "response.completed");
            let output = completed[0]["response"]["output"]
                .as_array()
                .expect("output items");
            let call = output
                .iter()
                .find(|item| item["call_id"] == id)
                .expect("completed call");
            assert_eq!(call["name"], name);
            let expected: Value = serde_json::from_str(args).expect("expected input");
            let actual: Value = serde_json::from_str(call["arguments"].as_str().unwrap()).unwrap();
            assert_eq!(actual, expected);
        }
        UpstreamProtocol::ChatCompletions => unreachable!(),
    }
}

fn assert_completed(events: &[Value], target: UpstreamProtocol) {
    let (start, terminal) = match target {
        asb_core::UpstreamProtocol::GeminiGenerateContent => panic!("Google native has a separate Claude fixture"),
        UpstreamProtocol::AnthropicMessages => {
            assert_eq!(events_of(events, "message_delta").len(), 1);
            ("message_start", "message_stop")
        }
        UpstreamProtocol::Responses => ("response.created", "response.completed"),
        UpstreamProtocol::ChatCompletions => unreachable!(),
    };
    assert_eq!(events_of(events, start).len(), 1);
    assert_eq!(events_of(events, terminal).len(), 1);
    assert_eq!(events.last().expect("terminal event")["type"], terminal);
}

struct Bytewise<'a>(&'a [u8]);

impl Read for Bytewise<'_> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = output.len().min(1);
        self.0.read(&mut output[..count])
    }
}

fn transcode(source: &str, target: UpstreamProtocol) -> (Vec<Value>, bool) {
    use crate::provider_diagnostics::{ProviderDiagnostic, ProviderFailureKind};

    let mut diagnostic = ProviderDiagnostic::new(
        ProviderFailureKind::StreamParse,
        "https://provider.example/chat/completions",
        "",
    );
    diagnostic.status = Some(200);
    diagnostic.request_id = Some("req-chat-fixture".to_string());
    let mut stream = SseTranscoder::new(
        Bytewise(source.as_bytes()),
        UpstreamProtocol::ChatCompletions,
        target,
        32 * 1024,
        None,
    )
    .expect("supported route")
    .with_diagnostics(diagnostic, &[]);
    let mut output = Vec::new();
    stream.read_to_end(&mut output).expect("transcoded stream");
    (decode_events(&output), stream.failed())
}
