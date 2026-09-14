//! Terminal response semantics shared by JSON and Responses streaming.

use super::{Map, StopReason, TransformError, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::gateway::transform) enum ResponsesTerminal {
    Completed,
    MaxTokens,
}

impl ResponsesTerminal {
    pub(in crate::gateway::transform) fn parse(
        response: &Map<String, Value>,
    ) -> Result<Self, TransformError> {
        ensure_no_error(response, "Responses response")?;
        match response.get("status").and_then(Value::as_str) {
            Some("completed") => {
                if response
                    .get("incomplete_details")
                    .is_some_and(|v| !v.is_null())
                {
                    return Err(TransformError(
                        "Responses completed response contains incomplete_details".into(),
                    ));
                }
                Ok(Self::Completed)
            }
            Some("incomplete") => match response
                .get("incomplete_details")
                .and_then(|details| details.get("reason"))
                .and_then(Value::as_str)
            {
                Some("max_output_tokens" | "max_tokens") => Ok(Self::MaxTokens),
                Some(reason) => Err(TransformError(format!(
                    "Responses incomplete response cannot finish safely: {reason}"
                ))),
                None => Err(TransformError(
                    "Responses incomplete response requires incomplete_details.reason".into(),
                )),
            },
            Some("failed" | "cancelled") => Err(upstream_error(response, "Responses response")),
            Some(status) => Err(TransformError(format!(
                "Responses response is not terminal: status={status}"
            ))),
            None => Err(TransformError(
                "Responses response.status must identify a terminal state".into(),
            )),
        }
    }

    pub(in crate::gateway::transform) fn stop_reason(self, has_tool_calls: bool) -> StopReason {
        match self {
            Self::MaxTokens => StopReason::MaxTokens,
            Self::Completed if has_tool_calls => StopReason::ToolUse,
            Self::Completed => StopReason::EndTurn,
        }
    }

    pub(in crate::gateway::transform) fn validate_item(
        self,
        item: &Map<String, Value>,
    ) -> Result<(), TransformError> {
        for field in ["id", "call_id", "name"] {
            if item
                .get(field)
                .is_some_and(|v| v.as_str().is_none_or(str::is_empty))
            {
                return Err(TransformError(format!(
                    "Responses output item.{field} must be a nonempty string"
                )));
            }
        }
        match item.get("status") {
            None | Some(Value::Null) => Ok(()),
            Some(Value::String(status)) if status == "completed" => Ok(()),
            Some(Value::String(status)) if status == "incomplete" && self == Self::MaxTokens => {
                Ok(())
            }
            Some(status) => Err(TransformError(format!(
                "Responses output item has a nonterminal or conflicting status: {status}"
            ))),
        }
    }
}

pub(in crate::gateway::transform) fn ensure_no_error(
    value: &Map<String, Value>,
    context: &str,
) -> Result<(), TransformError> {
    if value.get("error").is_some_and(|error| !error.is_null()) {
        Err(upstream_error(value, context))
    } else {
        Ok(())
    }
}

pub(in crate::gateway::transform) fn function_arguments(
    arguments: &str,
) -> Result<Value, TransformError> {
    let value: Value = serde_json::from_str(arguments).map_err(|_| {
        TransformError("Responses function_call.arguments must be valid JSON".into())
    })?;
    if !value.is_object() {
        return Err(TransformError(
            "Responses function_call.arguments must be a JSON object".into(),
        ));
    }
    Ok(value)
}

pub(in crate::gateway::transform) fn upstream_error(
    value: &Map<String, Value>,
    context: &str,
) -> TransformError {
    let error = value.get("error");
    let code = error
        .and_then(|error| error.get("code").or_else(|| error.get("type")))
        .or_else(|| value.get("code"))
        .and_then(Value::as_str);
    let message = error
        .and_then(|error| {
            error
                .get("message")
                .and_then(Value::as_str)
                .or_else(|| error.as_str())
        })
        .or_else(|| value.get("message").and_then(Value::as_str));
    let status = value
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("error");
    TransformError(format!(
        "{context} failed: status={status}, code={}, message={}",
        code.unwrap_or("upstream_error"),
        message
            .filter(|value| !value.trim().is_empty())
            .unwrap_or("generation did not complete"),
    ))
}
