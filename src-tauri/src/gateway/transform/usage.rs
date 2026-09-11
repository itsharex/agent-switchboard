//! One owner for provider usage parsing and the three protocol usage shapes.
//!
//! The three protocols report the same three counters under different names,
//! and real providers add their own cache counters on top of the documented
//! set. A usage object carries metering only, never request semantics, so an
//! unrecognised vendor counter is preserved where it has a canonical slot and
//! otherwise ignored. Rejecting a finished generation over a metering extra
//! would fail a request the user already paid for.

use super::{TransformError, Usage};
use asb_core::contracts::UpstreamProtocol;
use serde_json::{json, Map, Value};

pub(crate) fn parse(
    protocol: UpstreamProtocol,
    value: Option<&Value>,
) -> Result<Usage, TransformError> {
    let Some(value) = value else {
        return Ok(Usage::default());
    };
    let map = value
        .as_object()
        .ok_or_else(|| TransformError(format!("{} 必须是对象", context(protocol))))?;
    Ok(match protocol {
        UpstreamProtocol::Responses => parse_responses(map),
        UpstreamProtocol::ChatCompletions => parse_chat(map),
        UpstreamProtocol::AnthropicMessages => parse_anthropic(map),
    })
}

/// The Responses usage shape Codex reads back. Detail objects are emitted only
/// when the upstream actually reported them; an unknown counter is not turned
/// into a fabricated zero.
pub(crate) fn responses_json(usage: &Usage) -> Value {
    let mut value = totals(usage, "input_tokens", "output_tokens", "total_tokens");
    let object = value.as_object_mut().expect("usage object");
    if let Some(cached) = usage.cached_tokens {
        object.insert(
            "input_tokens_details".to_string(),
            json!({ "cached_tokens": cached }),
        );
    }
    if let Some(reasoning) = usage.reasoning_tokens {
        object.insert(
            "output_tokens_details".to_string(),
            json!({ "reasoning_tokens": reasoning }),
        );
    }
    value
}

pub(crate) fn chat_json(usage: &Usage) -> Value {
    let mut value = totals(usage, "prompt_tokens", "completion_tokens", "total_tokens");
    let object = value.as_object_mut().expect("usage object");
    if let Some(cached) = usage.cached_tokens {
        object.insert(
            "prompt_tokens_details".to_string(),
            json!({ "cached_tokens": cached }),
        );
    }
    if let Some(reasoning) = usage.reasoning_tokens {
        object.insert(
            "completion_tokens_details".to_string(),
            json!({ "reasoning_tokens": reasoning }),
        );
    }
    value
}

pub(crate) fn anthropic_json(usage: &Usage) -> Value {
    let mut value = anthropic_message_start_json(usage);
    value.as_object_mut().expect("usage object").insert(
        "output_tokens".to_string(),
        json!(usage.output_tokens.unwrap_or(0)),
    );
    value
}

/// Anthropic reports the input side on `message_start` and the output side on
/// `message_delta`, so a streaming bridge must be able to emit each half.
pub(crate) fn anthropic_message_start_json(usage: &Usage) -> Value {
    let mut value = json!({ "input_tokens": usage.input_tokens.unwrap_or(0), "output_tokens": 0 });
    if let Some(cached) = usage.cached_tokens {
        value
            .as_object_mut()
            .expect("usage object")
            .insert("cache_read_input_tokens".to_string(), json!(cached));
    }
    value
}

pub(crate) fn anthropic_message_delta_json(usage: &Usage) -> Value {
    json!({ "output_tokens": usage.output_tokens.unwrap_or(0) })
}

fn totals(usage: &Usage, input: &str, output: &str, total: &str) -> Value {
    let input_tokens = usage.input_tokens.unwrap_or(0);
    let output_tokens = usage.output_tokens.unwrap_or(0);
    json!({
        input: input_tokens,
        output: output_tokens,
        total: usage.total_tokens.unwrap_or(input_tokens + output_tokens),
    })
}

fn parse_responses(map: &Map<String, Value>) -> Usage {
    Usage {
        input_tokens: token(map, "input_tokens"),
        output_tokens: token(map, "output_tokens"),
        total_tokens: token(map, "total_tokens"),
        cached_tokens: pointer(map, "/input_tokens_details/cached_tokens"),
        reasoning_tokens: pointer(map, "/output_tokens_details/reasoning_tokens"),
    }
}

/// Chat Completions cache reporting is the least standardised part of the
/// ecosystem, so the documented detail object wins and each vendor's own
/// counter is a fallback behind it. DeepSeek reports `prompt_cache_hit_tokens`
/// and some relays only mirror it into the detail object.
fn parse_chat(map: &Map<String, Value>) -> Usage {
    Usage {
        input_tokens: token(map, "prompt_tokens"),
        output_tokens: token(map, "completion_tokens"),
        total_tokens: token(map, "total_tokens"),
        cached_tokens: pointer(map, "/prompt_tokens_details/cached_tokens")
            .or_else(|| pointer(map, "/input_tokens_details/cached_tokens"))
            .or_else(|| token(map, "prompt_cache_hit_tokens"))
            .or_else(|| token(map, "cache_read_input_tokens")),
        reasoning_tokens: pointer(map, "/completion_tokens_details/reasoning_tokens"),
    }
}

fn parse_anthropic(map: &Map<String, Value>) -> Usage {
    let input = token(map, "input_tokens");
    let output = token(map, "output_tokens");
    Usage {
        input_tokens: input,
        output_tokens: output,
        total_tokens: input.zip(output).map(|(input, output)| input + output),
        cached_tokens: token(map, "cache_read_input_tokens"),
        reasoning_tokens: None,
    }
}

fn token(map: &Map<String, Value>, key: &str) -> Option<u64> {
    map.get(key).and_then(Value::as_u64)
}

fn pointer(map: &Map<String, Value>, path: &str) -> Option<u64> {
    Value::Object(map.clone())
        .pointer(path)
        .and_then(Value::as_u64)
}

fn context(protocol: UpstreamProtocol) -> &'static str {
    match protocol {
        UpstreamProtocol::Responses => "Responses usage",
        UpstreamProtocol::ChatCompletions => "Chat usage",
        UpstreamProtocol::AnthropicMessages => "Anthropic usage",
    }
}
