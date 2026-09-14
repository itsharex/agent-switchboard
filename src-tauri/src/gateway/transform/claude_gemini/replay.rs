//! Backend-bound, authenticated replay of Gemini parts and thought signatures.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GeminiTurn {
    pub(super) parts: Vec<Value>,
    pub(super) call_ids: Vec<String>,
}

impl GeminiTurn {
    pub(crate) fn validate(&self) -> Result<(), TransformError> {
        let mut ids = BTreeSet::new();
        if self
            .call_ids
            .iter()
            .any(|id| id.is_empty() || !ids.insert(id))
        {
            return error("Gemini 工具调用标识为空或重复");
        }
        visible(&self.parts, &self.call_ids)?;
        Ok(())
    }

    pub(super) fn restore(&self, parts: &[Part]) -> Result<Vec<Value>, TransformError> {
        let actual = parts
            .iter()
            .filter_map(|part| match part {
                Part::Text(text) => Some(Ok(ResponsePart::Text(text.clone()))),
                Part::ToolCall {
                    id,
                    name,
                    namespace,
                    kind,
                    input,
                } => Some(Ok(ResponsePart::ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    namespace: namespace.clone(),
                    kind: *kind,
                    input: input.clone(),
                })),
                Part::Reasoning(_) => None,
                _ => Some(error("Gemini 签名消息中出现无法回放的内容")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        if projection(&actual) != projection(&visible(&self.parts, &self.call_ids)?) {
            return error("Gemini 签名消息已被修改，不能把原签名附到新的文本或工具调用上");
        }
        Ok(self.parts.clone())
    }
}

pub(super) fn visible(
    parts: &[Value],
    ids: &[String],
) -> Result<Vec<ResponsePart>, TransformError> {
    let mut output = Vec::new();
    let mut call = 0;
    for part in parts {
        let object = part
            .as_object()
            .ok_or_else(|| TransformError("Gemini part 必须是对象".into()))?;
        if let Some(signature) = object.get("thoughtSignature") {
            if signature.as_str().is_none_or(str::is_empty) {
                return error("Gemini thoughtSignature 无效");
            }
        }
        if object.get("thought").is_some_and(|v| !v.is_boolean()) {
            return error("Gemini thought 必须是布尔值");
        }
        if object.keys().any(|key| {
            !matches!(
                key.as_str(),
                "text" | "thought" | "thoughtSignature" | "functionCall"
            )
        }) {
            return error("Gemini 响应包含 Claude 无法表达的输出类型");
        }
        if let Some(text) = object.get("text") {
            let text = text
                .as_str()
                .ok_or_else(|| TransformError("Gemini text 必须是字符串".into()))?;
            if object.get("thought") != Some(&Value::Bool(true)) && !text.is_empty() {
                output.push(ResponsePart::Text(text.into()));
            }
        }
        if let Some(function) = object.get("functionCall") {
            if object.get("thought") == Some(&Value::Bool(true)) {
                return error("Gemini 工具调用不能标记为 thought");
            }
            let name = function
                .get("name")
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
                .ok_or_else(|| TransformError("Gemini functionCall 缺少 name".into()))?;
            let input = function.get("args").cloned().unwrap_or_else(|| json!({}));
            if !input.is_object() {
                return error("Gemini functionCall.args 必须是对象");
            }
            let id = ids
                .get(call)
                .ok_or_else(|| TransformError("Gemini 工具调用标识数量不一致".into()))?;
            output.push(ResponsePart::ToolCall {
                id: id.clone(),
                name: name.into(),
                namespace: None,
                kind: ToolKind::Function,
                input,
            });
            call += 1;
        }
        if !object.contains_key("text")
            && !object.contains_key("functionCall")
            && !object.contains_key("thoughtSignature")
        {
            return error("Gemini part 缺少可识别内容");
        }
    }
    if call != ids.len() {
        return error("Gemini 工具调用标识数量不一致");
    }
    Ok(output)
}

fn projection(parts: &[ResponsePart]) -> Value {
    let mut values = Vec::new();
    let mut text = String::new();
    for part in parts {
        match part {
            ResponsePart::Text(value) => text.push_str(value),
            ResponsePart::ToolCall {
                id, name, input, ..
            } => {
                if !text.is_empty() {
                    values.push(json!({"text":std::mem::take(&mut text)}));
                }
                values.push(json!({"id":id,"name":name,"input":input}));
            }
            ResponsePart::Reasoning(_) => {}
        }
    }
    if !text.is_empty() {
        values.push(json!({"text":text}));
    }
    Value::Array(values)
}

pub(super) fn call_id(part: &Value) -> Option<String> {
    part.get("functionCall").map(|call| {
        call.get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("asb_gemini_{}", uuid::Uuid::new_v4().simple()))
    })
}
