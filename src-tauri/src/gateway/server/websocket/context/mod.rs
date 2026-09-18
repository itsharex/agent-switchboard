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
    observed_route: Option<String>,
    // After a route change, opaque input must come from the current bound cache.
    require_known_continuation: bool,
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
    pub(super) fn for_route(route_revision: &str) -> Self {
        Self {
            observed_route: Some(route_revision.to_string()),
            ..Self::default()
        }
    }

    pub(super) fn prepare(
        &mut self,
        route_revision: &str,
        mode: ResponsesRequestMode,
        text: &str,
    ) -> Result<PreparedResponse, ContextError> {
        self.validate_wire_binding(route_revision, text)?;
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
        self.validate_continuation(&binding, &input, previous_response_id.as_deref())?;
        let full_input = if previous_response_id.is_some() {
            let previous = self.latest.as_ref().expect("validated previous response");
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
            return self.prepare_prewarm(model, binding, template, full_input);
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

    fn validate_wire_binding(
        &mut self,
        route_revision: &str,
        text: &str,
    ) -> Result<(), ContextError> {
        let value: Value = serde_json::from_str(text)
            .map_err(|_| ContextError::invalid("Codex WebSocket 请求不是有效 JSON"))?;
        let root = object(&value, "Codex WebSocket 请求")?;
        let binding = ConversationBinding {
            route_revision: route_revision.to_string(),
            session_id: session_id(&value)?,
        };
        let previous = optional_nonempty_string(root, "previous_response_id")?;
        let input = root
            .get("input")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        self.validate_continuation(&binding, input, previous.as_deref())
    }

    fn validate_continuation(
        &mut self,
        binding: &ConversationBinding,
        input: &[Value],
        previous_response_id: Option<&str>,
    ) -> Result<(), ContextError> {
        self.require_known_continuation |= self
            .observed_route
            .as_ref()
            .is_some_and(|route| route != &binding.route_revision)
            || self
                .latest
                .as_ref()
                .is_some_and(|previous| previous.binding != *binding);
        self.observed_route = Some(binding.route_revision.clone());
        if let Some(id) = previous_response_id {
            let previous = self
                .latest
                .as_ref()
                .ok_or_else(ContextError::missing_previous)?;
            if previous.id != id || previous.binding != *binding {
                return Err(ContextError::stale_continuation());
            }
        }
        if self.require_known_continuation
            && input
                .iter()
                .filter(|item| is_opaque_continuation(item))
                .any(|item| !self.known_continuation(binding, item))
        {
            return Err(ContextError::stale_continuation());
        }
        Ok(())
    }

    fn known_continuation(&self, binding: &ConversationBinding, item: &Value) -> bool {
        self.latest
            .as_ref()
            .filter(|previous| previous.binding == *binding)
            .is_some_and(|previous| {
                previous
                    .full_input
                    .iter()
                    .chain(&previous.output)
                    .any(|known| {
                        known.get("type") == item.get("type")
                            && match item.get("encrypted_content") {
                                Some(payload) => known.get("encrypted_content") == Some(payload),
                                None => known == item,
                            }
                    })
            })
    }

    fn prepare_prewarm(
        &mut self,
        model: String,
        binding: ConversationBinding,
        template: Map<String, Value>,
        full_input: Vec<Value>,
    ) -> Result<PreparedResponse, ContextError> {
        if !full_input.is_empty() {
            return Err(ContextError::invalid(
                "generate=false 预热请求的 input 必须为空数组",
            ));
        }
        let id = format!("asb_prewarm_{}", Uuid::new_v4().simple());
        let response = json!({
            "id":id, "object":"response", "status":"completed", "model":model, "output":[],
            "usage":{"input_tokens":0, "output_tokens":0, "total_tokens":0}, "error":null,
        });
        self.latest = Some(CachedResponse {
            id,
            binding,
            template,
            full_input,
            output: vec![],
        });
        Ok(PreparedResponse::Prewarm(PrewarmResponse {
            value: response,
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

fn is_opaque_continuation(item: &Value) -> bool {
    match item.get("type").and_then(Value::as_str) {
        Some("reasoning") => item.get("encrypted_content").is_some(),
        Some("compaction" | "context_compaction" | "item_reference") => true,
        _ => false,
    }
}

mod completed;
mod input;
mod json;
mod native;
mod normalize;


use completed::normalize_completed_response;
use input::normalize_input;
use json::*;
use normalize::*;
