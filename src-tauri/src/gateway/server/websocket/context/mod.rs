//! Per-WebSocket Responses state. Codex sends incremental request input after
//! the first turn, so the gateway owns only the immediately preceding
//! completed response in this connection and reconstructs its visible context
//! before crossing protocol boundaries.

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
    pub(super) fn prepare(&mut self, text: &str) -> Result<PreparedResponse, ContextError> {
        let normalized = normalize_request(text)?;
        let full_input = if let Some(previous_response_id) = normalized.previous_response_id {
            let Some(previous) = self.latest.as_ref() else {
                return Err(ContextError::missing_previous());
            };
            if previous.id != previous_response_id || previous.template != normalized.template {
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
        let (id, output) = normalize_completed_response(response)?;
        self.latest = Some(CachedResponse {
            id,
            template: request.template.clone(),
            full_input: request.full_input.clone(),
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

mod json;
mod normalize;

#[cfg(test)]
mod tests;

use json::*;
use normalize::*;
