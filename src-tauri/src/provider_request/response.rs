use asb_core::contracts::UpstreamProtocol;
use serde_json::Value;

pub(super) struct Reply {
    pub text: String,
    pub model: Option<String>,
}

pub(super) fn parse(protocol: UpstreamProtocol, body: &[u8]) -> Result<Reply, String> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|_| "供应商响应不是有效 JSON，未收到可验证的模型回复".to_string())?;
    if value.get("error").is_some_and(|error| !error.is_null()) {
        return Err(
            error_message(&value).unwrap_or_else(|| "供应商返回错误，未完成模型请求".to_string())
        );
    }
    let text = match protocol {
        UpstreamProtocol::Responses => responses_text(&value),
        UpstreamProtocol::ChatCompletions => chat_text(&value),
        UpstreamProtocol::AnthropicMessages => anthropic_text(&value),
        UpstreamProtocol::GeminiGenerateContent => super::claude::gemini_test_text(&value),
    }?;
    if text.trim().is_empty() {
        return Err("响应没有非空模型文本；仅推理或空响应不能验证请求成功".to_string());
    }
    Ok(Reply {
        text,
        model: value
            .get(if protocol == UpstreamProtocol::GeminiGenerateContent { "modelVersion" } else { "model" })
            .and_then(Value::as_str)
            .filter(|model| !model.trim().is_empty())
            .map(str::to_string),
    })
}

fn responses_text(value: &Value) -> Result<String, String> {
    if value["object"] != "response" {
        return Err("响应不符合 OpenAI Responses 格式".to_string());
    }
    if value["status"] != "completed"
        || value
            .get("incomplete_details")
            .is_some_and(|v| !v.is_null())
    {
        return Err(incomplete());
    }
    let output = value["output"]
        .as_array()
        .ok_or_else(|| "响应缺少 output 数组".to_string())?;
    let mut text = String::new();
    for item in output {
        if item["type"] != "message" {
            continue;
        }
        if item["role"] != "assistant" || item["status"] != "completed" {
            return Err(incomplete());
        }
        let content = item["content"]
            .as_array()
            .ok_or_else(|| "模型消息缺少 content 数组".to_string())?;
        for part in content {
            if part["type"] == "output_text" {
                text.push_str(
                    part["text"]
                        .as_str()
                        .ok_or_else(|| "模型文本字段无效".to_string())?,
                );
            }
        }
    }
    Ok(text)
}

fn chat_text(value: &Value) -> Result<String, String> {
    if value["object"] != "chat.completion" {
        return Err("响应不符合 Chat Completions 格式".to_string());
    }
    let choice = value["choices"]
        .as_array()
        .and_then(|choices| choices.first())
        .ok_or_else(|| "响应缺少模型回复 choices".to_string())?;
    if choice["finish_reason"] != "stop" {
        return Err(incomplete());
    }
    let message = &choice["message"];
    if message["role"] != "assistant" {
        return Err("响应没有 assistant 模型消息".to_string());
    }
    message["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "响应没有模型文本；仅推理或工具调用不能验证请求成功".to_string())
}

fn anthropic_text(value: &Value) -> Result<String, String> {
    if value["type"] != "message" || value["role"] != "assistant" {
        return Err("响应不符合 Anthropic Messages 格式".to_string());
    }
    if !matches!(
        value["stop_reason"].as_str(),
        Some("end_turn" | "stop_sequence")
    ) {
        return Err(incomplete());
    }
    let content = value["content"]
        .as_array()
        .ok_or_else(|| "响应缺少 content 数组".to_string())?;
    let mut text = String::new();
    for part in content {
        if part["type"] == "text" {
            text.push_str(
                part["text"]
                    .as_str()
                    .ok_or_else(|| "模型文本字段无效".to_string())?,
            );
        }
    }
    Ok(text)
}

fn incomplete() -> String {
    format!(
        "供应商未返回完整模型回复；本次请求输出上限为 {} tokens，请检查模型是否需要更多推理预算，或使用其他可用模型重试",
        super::contracts::MAX_OUTPUT_TOKENS
    )
}

pub(super) fn error_message(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .filter(|message| !message.trim().is_empty())
        .map(str::to_string)
}

pub(super) fn redact(text: String, api_key: &str) -> String {
    if api_key.is_empty() {
        text
    } else {
        text.replace(api_key, asb_core::redact::REDACTED)
    }
}

#[cfg(test)]
mod tests;
