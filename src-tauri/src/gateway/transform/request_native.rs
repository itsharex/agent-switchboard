//! Same-protocol transport preserves vendor bodies, not ASB continuation envelopes.
use super::*;
pub(super) fn convert(
    protocol: UpstreamProtocol,
    mut value: Value,
    body: &[u8],
    transport: Option<&ReasoningTransport>,
) -> Result<ConvertedRequest, TransformError> {
    if protocol == UpstreamProtocol::AnthropicMessages {
        reject_gateway_reasoning(&value)?;
    }
    let expanded = if protocol == UpstreamProtocol::Responses {
        crate::gateway::compaction::expand_input(
            &mut value,
            transport.map(|transport| transport.continuation_key()),
        )?
    } else {
        false
    };
    let body = if expanded {
        serde_json::to_vec(&value)
            .map_err(|_| TransformError("无法编码原生 Responses 请求".into()))?
    } else {
        body.to_vec()
    };
    Ok(ConvertedRequest {
        stream: value
            .get("stream")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        body,
    })
}
fn reject_gateway_reasoning(value: &Value) -> Result<(), TransformError> {
    for message in value
        .get("messages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for part in message
            .get("content")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let payload = match part.get("type").and_then(Value::as_str) {
                Some("thinking") => part.get("signature"),
                Some("redacted_thinking") => part.get("data"),
                _ => None,
            };
            if payload
                .and_then(Value::as_str)
                .is_some_and(ReasoningTransport::is_continuation)
            {
                return Err(TransformError(
                    "本机推理续接不能直接发送到另一个 Anthropic 上游；请从原供应商继续会话".into(),
                ));
            }
        }
    }
    Ok(())
}
