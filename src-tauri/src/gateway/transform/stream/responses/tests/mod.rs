use super::*;
use crate::gateway::transform::convert_response;

mod assertions;
mod consistency;
mod custom;
mod failures;
mod lifecycle;
mod native;
mod reader;

use assertions::*;

struct Harness {
    decoder: ResponsesToAnthropic,
    events: Vec<Value>,
}

impl Harness {
    fn new() -> Self {
        Self {
            decoder: ResponsesToAnthropic::new(Some(transport())),
            events: Vec::new(),
        }
    }

    fn push(&mut self, value: Value) -> Result<(), TransformError> {
        let event = value["type"].as_str().unwrap().to_string();
        self.raw(Some(event), value.to_string())
    }

    fn raw(&mut self, event: Option<String>, data: String) -> Result<(), TransformError> {
        let bytes = self.decoder.on_frame(Frame { event, data })?;
        self.collect(bytes);
        Ok(())
    }

    fn collect(&mut self, bytes: Vec<u8>) {
        for line in String::from_utf8(bytes).unwrap().lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                self.events.push(serde_json::from_str(data).unwrap());
            }
        }
    }

    fn begin(&mut self) {
        self.push(json!({"type":"response.created","response":{
            "id":"resp_lifecycle","model":"test-model","status":"in_progress","output":[]
        }}))
        .unwrap();
    }

    fn finish(&mut self) -> Result<(), TransformError> {
        let bytes = self.decoder.finish()?;
        self.collect(bytes);
        Ok(())
    }

    fn terminal(&mut self, value: Value) -> Result<(), TransformError> {
        let event = if value["status"] == "incomplete" {
            "response.incomplete"
        } else {
            "response.completed"
        };
        self.push(json!({"type":event,"response":value}))
    }
}

fn transport() -> ReasoningTransport {
    ReasoningTransport::from_continuation_key([81; 32])
}

fn response(output: Vec<Value>) -> Value {
    json!({"id":"resp_lifecycle","object":"response","model":"test-model",
        "status":"completed","output":output,"error":null,"incomplete_details":null,
        "usage":{"input_tokens":12,"output_tokens":5,"input_tokens_details":{"cached_tokens":7}}})
}

fn limited(output: Vec<Value>) -> Value {
    let mut value = response(output);
    value["status"] = json!("incomplete");
    value["incomplete_details"] = json!({"reason":"max_output_tokens"});
    value
}

fn text_item(id: &str, text: &str) -> Value {
    json!({"type":"message","id":id,"role":"assistant","status":"completed",
        "content":[{"type":"output_text","text":text,"annotations":[]}]})
}

fn tool_item(index: u64, arguments: &str) -> Value {
    json!({"type":"function_call","id":format!("fc_{index}"),"call_id":format!("call_{index}"),
        "name":"Read","arguments":arguments,"status":"completed"})
}

fn added(index: u64, mut item: Value) -> Value {
    item["status"] = json!("in_progress");
    match item["type"].as_str().unwrap() {
        "message" => item["content"] = json!([]),
        "function_call" => item["arguments"] = json!(""),
        "reasoning" => {
            item["summary"] = json!([]);
            item.as_object_mut().unwrap().remove("encrypted_content");
        }
        _ => {}
    }
    json!({"type":"response.output_item.added","output_index":index,"item":item})
}

fn done(index: u64, item: Value) -> Value {
    json!({"type":"response.output_item.done","output_index":index,"item":item})
}

fn text_delta(index: u64, id: &str, text: &str) -> Value {
    json!({"type":"response.output_text.delta","output_index":index,"item_id":id,
        "content_index":0,"delta":text})
}

fn arguments_delta(index: u64, arguments: &str) -> Value {
    json!({"type":"response.function_call_arguments.delta","output_index":index,
        "item_id":format!("fc_{index}"),"delta":arguments})
}
