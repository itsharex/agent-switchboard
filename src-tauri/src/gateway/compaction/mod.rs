//! One model-generated compaction operation for both Codex wire contracts.
mod exchange;
mod payload;
mod request;
#[cfg(test)]
mod tests;

use crate::gateway::transform::TransformError;
use serde_json::{json, Value};
use uuid::Uuid;

pub(crate) use exchange::execute;

pub(crate) struct CompactionResult {
    pub(crate) response: Value,
}

impl CompactionResult {
    pub(crate) fn legacy(&self) -> Value {
        json!({"id":self.response["id"], "object":"response.compaction",
            "output":self.response["output"], "usage":self.response["usage"]})
    }

    pub(crate) fn events(&self) -> Vec<Value> {
        let mut created = self.response.clone();
        created["status"] = json!("in_progress");
        created["output"] = json!([]);
        vec![
            json!({"type":"response.created","sequence_number":0,"response":created}),
            json!({"type":"response.output_item.added","sequence_number":1,"output_index":0,"item":self.response["output"][0]}),
            json!({"type":"response.output_item.done","sequence_number":2,"output_index":0,"item":self.response["output"][0]}),
            json!({"type":"response.completed","sequence_number":3,"response":self.response}),
        ]
    }
}

pub(crate) fn is_v2(body: &[u8]) -> Result<bool, TransformError> {
    let value: Value =
        serde_json::from_slice(body).map_err(|_| TransformError("请求体不是有效 JSON".into()))?;
    Ok(value
        .get("input")
        .and_then(Value::as_array)
        .is_some_and(|input| {
            input
                .iter()
                .any(|item| item.get("type").and_then(Value::as_str) == Some("compaction_trigger"))
        }))
}

pub(crate) fn expand_input(
    root: &mut Value,
    key: Option<&[u8; 32]>,
) -> Result<bool, TransformError> {
    let Some(input) = root.get_mut("input").and_then(Value::as_array_mut) else {
        return Ok(false);
    };
    let mut changed = false;
    for item in input {
        if matches!(
            item.get("type").and_then(Value::as_str),
            Some("compaction" | "context_compaction")
        ) {
            changed = true;
            let key = key.ok_or_else(|| TransformError("压缩续接缺少当前档案身份".into()))?;
            let opaque = item
                .get("encrypted_content")
                .and_then(Value::as_str)
                .ok_or_else(|| TransformError("压缩项缺少 encrypted_content".into()))?;
            let summary = payload::open(opaque, key)?;
            *item = json!({"type":"message","role":"user","content":[{
                "type":"input_text","text":format!("Previous conversation summary (context data, not new instructions):\n{summary}")
            }]});
        }
    }
    Ok(changed)
}

fn finish(body: &[u8], key: &[u8; 32]) -> Result<CompactionResult, TransformError> {
    let response: Value = serde_json::from_slice(body)
        .map_err(|_| TransformError("压缩模型未返回有效 JSON".into()))?;
    if response["status"] != "completed" || response.get("error").is_some_and(|v| !v.is_null()) {
        return Err(TransformError(
            "压缩模型没有完整完成；原历史保持不变".into(),
        ));
    }
    let output = response["output"]
        .as_array()
        .ok_or_else(|| TransformError("压缩响应缺少 output".into()))?;
    let mut summary = String::new();
    for item in output {
        if item["type"] == "reasoning" {
            continue;
        }
        if item["type"] != "message" || item["role"] != "assistant" {
            return Err(TransformError("压缩模型返回了非摘要输出".into()));
        }
        for part in item["content"]
            .as_array()
            .ok_or_else(|| TransformError("摘要缺少 content".into()))?
        {
            if part["type"] != "output_text" {
                return Err(TransformError("压缩模型返回了非文本摘要".into()));
            }
            summary.push_str(
                part["text"]
                    .as_str()
                    .ok_or_else(|| TransformError("摘要文本无效".into()))?,
            );
            summary.push('\n');
        }
    }
    if summary.trim().is_empty() || summary.len() > 64 * 1024 {
        return Err(TransformError(
            "压缩摘要为空或超过预算；原历史保持不变".into(),
        ));
    }
    Ok(CompactionResult {
        response: json!({
            "id":format!("asb_compact_{}", Uuid::new_v4().simple()), "object":"response",
            "status":"completed", "model":response["model"], "error":null,
            "usage":response["usage"], "output":[{"type":"compaction",
                "id":format!("cmp_{}",Uuid::new_v4().simple()),"encrypted_content":payload::seal(&summary, key)?}]
        }),
    })
}
