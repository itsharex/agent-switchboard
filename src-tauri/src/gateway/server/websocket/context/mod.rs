//! Per-WebSocket Responses state. Codex sends incremental request input after
//! the first turn, so the gateway owns only the immediately preceding
//! completed response in this connection, binds it to its route revision and
//! client session, and reconstructs visible context before crossing protocols.

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
    binding: ConversationBinding,
    template: Map<String, Value>,
    full_input: Vec<Value>,
    output: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ConversationBinding {
    route_revision: String,
    session_id: Option<String>,
}

#[derive(Debug)]
pub(super) struct PendingRequest {
    pub(super) body: Vec<u8>,
    pub(super) stream: bool,
    binding: ConversationBinding,
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
            message: "Previous response was not found. Retry the full request.".to_string(),
        }
    }

    fn stale_continuation() -> Self {
        Self {
            code: "previous_response_not_found",
            message: "Previous response or encrypted continuation belongs to a different provider route or session. Retry the full request with visible input and without opaque continuation state.".to_string(),
        }
    }
}

impl ConversationContext {
    pub(super) fn prepare(
        &mut self,
        route_revision: &str,
        mode: ResponsesRequestMode,
        text: &str,
    ) -> Result<PreparedResponse, ContextError> {
        self.prepare_normalized(route_revision, normalize_request(mode, text)?, false)
    }

    pub(super) fn prepare_native(
        &mut self,
        route_revision: &str,
        text: &str,
    ) -> Result<PreparedResponse, ContextError> {
        self.prepare_normalized(route_revision, native::normalize(text)?, true)
    }

    fn prepare_normalized(
        &mut self,
        route_revision: &str,
        normalized: NormalizedRequest,
        native: bool,
    ) -> Result<PreparedResponse, ContextError> {
        let NormalizedRequest {
            model,
            template,
            input,
            stream,
            generate,
            previous_response_id,
            session_id,
        } = normalized;
        let binding = ConversationBinding {
            route_revision: route_revision.to_string(),
            session_id,
        };
        if previous_response_id.is_none()
            && contains_opaque_continuation(&input)
            && self
                .latest
                .as_ref()
                .is_some_and(|previous| previous.binding != binding)
        {
            return Err(ContextError::stale_continuation());
        }
        let full_input = if let Some(previous_response_id) = previous_response_id {
            let Some(previous) = self.latest.as_ref() else {
                return Err(ContextError::missing_previous());
            };
            if previous.id != previous_response_id || previous.binding != binding {
                return Err(ContextError::stale_continuation());
            }
            if (!native && previous.template != template)
                || (native && previous.template.get("model") != template.get("model"))
            {
                return Err(ContextError::missing_previous());
            }
            let mut full_input = previous.full_input.clone();
            full_input.extend(previous.output.clone());
            full_input.extend(input);
            full_input
        } else {
            input
        };

        if !generate {
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
                "model": model,
                "output": [],
                "usage": { "input_tokens": 0, "output_tokens": 0, "total_tokens": 0 },
                "error": null,
            });
            self.latest = Some(CachedResponse {
                id,
                binding,
                template,
                full_input,
                output: vec![],
            });
            return Ok(PreparedResponse::Prewarm(PrewarmResponse {
                value: response,
            }));
        }

        let mut root = template.clone();
        root.insert("input".to_string(), Value::Array(full_input.clone()));
        let body = serde_json::to_vec(&Value::Object(root))
            .map_err(|_| ContextError::invalid("无法编码 Codex WebSocket 请求"))?;
        Ok(PreparedResponse::Upstream(PendingRequest {
            body,
            stream,
            binding,
            native,
            template,
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
            binding: request.binding.clone(),
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
    session_id: Option<String>,
}

fn session_id(value: &Value) -> Result<Option<String>, ContextError> {
    let root = object(value, "Codex WebSocket 请求")?;
    let Some(metadata) = root.get("client_metadata") else {
        return Ok(None);
    };
    let metadata = object(metadata, "client_metadata")?;
    metadata
        .get("session_id")
        .map(|value| nonempty_string(value, "client_metadata.session_id"))
        .transpose()
}

fn contains_opaque_continuation(input: &[Value]) -> bool {
    input.iter().any(|item| {
        let Some(item) = item.as_object() else {
            return false;
        };
        matches!(
            item.get("type").and_then(Value::as_str),
            Some("reasoning" | "compaction")
        ) && item.contains_key("encrypted_content")
    })
}

mod completed;
mod input;
mod json;
mod native;
mod normalize;

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod native_tests;
#[cfg(test)]
mod tests;

use completed::normalize_completed_response;
use input::normalize_input;
use json::*;
use normalize::*;
