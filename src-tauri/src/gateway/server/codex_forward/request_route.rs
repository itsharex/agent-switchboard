use super::*;
use asb_core::contracts::CodexSubagentRoute;

#[cfg(test)]
mod tests;

/// Resolve the provider before protocol normalization, operation admission or
/// continuation binding. All transports consume the same target snapshot.
pub(in crate::gateway::server) fn resolve_request_route(
    inner: &GatewayInner,
    primary: ActiveRoute,
    candidates: Vec<ActiveRoute>,
    operation: CodexOperation,
    body: Vec<u8>,
) -> Result<(ActiveRoute, Vec<ActiveRoute>, Vec<u8>), (u16, String)> {
    let mut value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|_| (422, "Codex 请求体不是有效 JSON".to_string()))?;
    let routed = value.get("model").and_then(serde_json::Value::as_str)
        .filter(|model| model.starts_with(CodexSubagentRoute::WIRE_PREFIX));
    let (route, candidates, body) = if let Some(model) = routed {
        if !operation.is_responses() && !operation.is_compact() {
            return Err((422, "此操作不支持跨供应商子代理模型".into()));
        }
        let wire = CodexSubagentRoute::parse_wire_id(model)
            .filter(CodexSubagentRoute::has_canonical_profile_id)
            .ok_or_else(|| (422, format!("请求模型 {model} 不是有效的子代理路由引用")))?;
        let (route, variants) = inner.subagent_route_candidates(&wire)
            .map_err(|message| (422, message))?;
        value["model"] = serde_json::Value::String(wire.model);
        let body = serde_json::to_vec(&value)
            .map_err(|_| (422, "Codex 请求序列化失败".to_string()))?;
        (route, variants, body)
    } else {
        (primary, candidates, body)
    };
    super::super::codex::ensure_operation(&route, operation)
        .map_err(|message| (501, message))?;
    Ok((route, candidates, body))
}
