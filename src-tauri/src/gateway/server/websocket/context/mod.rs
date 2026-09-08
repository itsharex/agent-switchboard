//! Per-WebSocket Responses state. Codex sends incremental request input after
//! the first turn, so the gateway owns only the immediately preceding
//! completed response in this connection and reconstructs its visible context
//! before crossing protocol boundaries.

use asb_core::contracts::ResponsesRequestMode;
use serde_json::{json, Map, Value};
use uuid::Uuid;

#[derive(Default)]
pub(super) struct ConversationContext {
    latest: Option<CachedResponse>,
}

#[derive(Clone)]
struct CachedResponse {
    id: String,
    template: Map<String, Value>,
    full_input: Vec<Value>,
    output: Vec<Value>,
}

#[derive(Debug)]
pub(super) struct PendingRequest {
    pub(super) body: Vec<u8>,
    pub(super) stream: bool,
    native: bool,
    template: Map<String, Value>,
    full_input: Vec<Value>,
}

#[derive(Debug)]
pub(super) struct PrewarmResponse {
    pub(super) value: Value,
}

#[derive(Debug)]
pub(super) enum PreparedResponse {
    Upstream(PendingRequest),
    Prewarm(PrewarmResponse),
}

#[derive(Debug)]
pub(super) struct ContextError {
    pub(super) code: &'static str,
    pub(super) message: String,
}

impl ContextError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_request",
            message: message.into(),
        }
    }

    fn missing_previous() -> Self {
        Self {
            code: "previous_response_not_found",
            message: "Previous response was not found. Retrying the full request.".to_string(),
        }
    }
}

impl ConversationContext {
    pub(super) fn prepare(
        &mut self,
        mode: ResponsesRequestMode,
        text: &str,
    ) -> Result<PreparedResponse, ContextError> {
        self.prepare_normalized(normalize_request(mode, text)?, false)
    }

    pub(super) fn prepare_native(&mut self, text: &str) -> Result<PreparedResponse, ContextError> {
        self.prepare_normalized(native::normalize(text)?, true)
    }

    fn prepare_normalized(
        &mut self,
        normalized: NormalizedRequest,
        native: bool,
    ) -> Result<PreparedResponse, ContextError> {
        let full_input = if let Some(previous_response_id) = normalized.previous_response_id {
            let Some(previous) = self.latest.as_ref() else {
                return Err(ContextError::missing_previous());
            };
            if previous.id != previous_response_id
                || (!native && previous.template != normalized.template)
                || (native && previous.template.get("model") != normalized.template.get("model"))
            {
                return Err(ContextError::missing_previous());
            }
            let mut input = previous.full_input.clone();
            input.extend(previous.output.clone());
            input.extend(normalized.input);
            input
        } else {
            normalized.input
        };

        if !normalized.generate {
            if !full_input.is_empty() {
                return Err(ContextError::invalid(
                    "generate=false 预热请求的 input 必须为空数组",
                ));
            }
            let id = format!("asb_prewarm_{}", Uuid::new_v4().simple());
            let response = json!({
                "id": id,
                "object": "response",
                "status": "completed",
                "model": normalized.model,
                "output": [],
                "usage": { "input_tokens": 0, "output_tokens": 0, "total_tokens": 0 },
                "error": null,
            });
            self.latest = Some(CachedResponse {
                id,
                template: normalized.template,
                full_input,
                output: vec![],
            });
            return Ok(PreparedResponse::Prewarm(PrewarmResponse {
                value: response,
            }));
        }

        let mut root = normalized.template.clone();
        root.insert("input".to_string(), Value::Array(full_input.clone()));
        let body = serde_json::to_vec(&Value::Object(root))
            .map_err(|_| ContextError::invalid("无法编码 Codex WebSocket 请求"))?;
        Ok(PreparedResponse::Upstream(PendingRequest {
            body,
            stream: normalized.stream,
            native,
            template: normalized.template,
            full_input,
        }))
    }

    /// Records a completed Responses response after normalizing only the
    /// visible output and the gateway-owned opaque reasoning continuation.
    pub(super) fn record_completed(
        &mut self,
        request: &PendingRequest,
        response: &Value,
    ) -> Result<(), ContextError> {
        let (id, output) = if request.native {
            native::completed(response)?
        } else {
            normalize_completed_response(response)?
        };
        self.latest = Some(CachedResponse {
            id,
            template: request.template.clone(),
            full_input: if output.iter().any(|item| item["type"] == "compaction") {
                Vec::new()
            } else {
                request.full_input.clone()
            },
            output,
        });
        Ok(())
    }
}

struct NormalizedRequest {
    model: String,
    template: Map<String, Value>,
    input: Vec<Value>,
    stream: bool,
    generate: bool,
    previous_response_id: Option<String>,
}

mod completed;
mod input;
mod json;
mod native;
mod normalize;

#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod tests;

use completed::normalize_completed_response;
use input::normalize_input;
use json::*;
use normalize::*;
