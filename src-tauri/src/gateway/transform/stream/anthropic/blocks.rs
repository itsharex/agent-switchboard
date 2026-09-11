//! Content-block handlers for the Anthropic-to-Responses conversion.

use super::*;

impl AnthropicToResponses {
    pub(super) fn block_start(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        self.require_started()?;
        let value = json_data(frame, "content_block_start")?;
        let map = object(&value, "content_block_start")?;
        allowed(
            map,
            &["type", "index", "content_block"],
            "content_block_start",
        )?;
        if required_string(map, "type", "content_block_start")? != "content_block_start" {
            return Err(TransformError("content_block_start.type 无效".to_string()));
        }
        let index = required_index(map, "content_block_start")?;
        if self.blocks.contains_key(&index) {
            return Err(TransformError(
                "Anthropic SSE 重复 content block 索引".to_string(),
            ));
        }
        let block = object(
            map.get("content_block").ok_or_else(|| {
                TransformError("content_block_start 缺少 content_block".to_string())
            })?,
            "content_block_start.content_block",
        )?;
        let kind = required_string(block, "type", "content_block")?;
        match kind.as_str() {
            "text" => {
                allowed(block, &["type", "text"], "Anthropic text block")?;
                // Anthropic starts ordinary text blocks with an empty text
                // value, then sends the first token in `text_delta`.
                let text = block
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        TransformError("Anthropic text block.text 必须是字符串".to_string())
                    })?;
                let id = self.id.as_deref().expect("started");
                append_event(
                    output,
                    "response.output_item.added",
                    json!({
                        "type": "response.output_item.added",
                        "output_index": index,
                        "item": {
                            "type": "message",
                            "id": format!("msg_{id}_{index}"),
                            "status": "in_progress",
                            "role": "assistant",
                            "content": [],
                        },
                    }),
                );
                append_event(
                    output,
                    "response.content_part.added",
                    json!({
                        "type": "response.content_part.added",
                        "output_index": index,
                        "content_index": 0,
                        "part": { "type": "output_text", "text": "", "annotations": [] },
                    }),
                );
                if !text.is_empty() {
                    append_event(
                        output,
                        "response.output_text.delta",
                        json!({
                            "type": "response.output_text.delta",
                            "output_index": index,
                            "content_index": 0,
                            "delta": text,
                        }),
                    );
                }
                self.blocks.insert(
                    index,
                    Block::Text {
                        text,
                        stopped: false,
                    },
                );
            }
            "thinking" => {
                allowed(
                    block,
                    &["type", "thinking", "signature"],
                    "Anthropic thinking block",
                )?;
                // Anthropic signs the finished trace, so the signature is kept
                // for a verbatim replay on the next turn.
                let signature = match block.get("signature") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(value)) if value.is_empty() => None,
                    Some(Value::String(value)) => Some(value.clone()),
                    Some(_) => {
                        return Err(TransformError(
                            "Anthropic thinking block.signature 必须是字符串".to_string(),
                        ))
                    }
                };
                // Reasoning is released to the client exactly once, as an
                // opaque continuation item, so the readable trace stays local
                // until this block closes.
                let text = block
                    .get("thinking")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        TransformError("Anthropic thinking block.thinking 必须是字符串".to_string())
                    })?;
                self.blocks.insert(
                    index,
                    Block::Thinking {
                        text,
                        signature,
                        stopped: false,
                        reasoning: None,
                    },
                );
            }
            "redacted_thinking" => {
                allowed(block, &["type", "data"], "Anthropic redacted_thinking")?;
                let data = block
                    .get("data")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        TransformError(
                            "Anthropic redacted_thinking.data 必须是非空字符串".to_string(),
                        )
                    })?;
                self.blocks.insert(
                    index,
                    Block::Redacted {
                        data,
                        stopped: false,
                        reasoning: None,
                    },
                );
            }
            "tool_use" => {
                allowed(
                    block,
                    &["type", "id", "name", "input"],
                    "Anthropic tool block",
                )?;
                let input = block
                    .get("input")
                    .and_then(Value::as_object)
                    .ok_or_else(|| {
                        TransformError("Anthropic tool block.input 必须是对象".to_string())
                    })?;
                if !input.is_empty() {
                    return Err(TransformError(
                        "Anthropic 流式 tool_use 初始 input 必须为空对象".to_string(),
                    ));
                }
                let id = required_string(block, "id", "Anthropic tool block")?;
                let encoded_name = required_string(block, "name", "Anthropic tool block")?;
                let (namespace, name, kind) = parse_target_name(&encoded_name)?;
                append_event(
                    output,
                    "response.output_item.added",
                    json!({
                        "type": "response.output_item.added",
                        "output_index": index,
                        "item": responses_tool_call_item(
                            &id,
                            &name,
                            namespace.as_deref(),
                            kind,
                            "",
                            "in_progress",
                        )?,
                    }),
                );
                self.blocks.insert(
                    index,
                    Block::Tool {
                        id,
                        name,
                        namespace,
                        kind,
                        arguments: String::new(),
                        stopped: false,
                    },
                );
            }
            other => {
                return Err(TransformError(format!(
                    "Anthropic SSE content block {other} 不支持转换"
                )))
            }
        }
        Ok(())
    }

    pub(super) fn block_delta(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        self.require_started()?;
        let value = json_data(frame, "content_block_delta")?;
        let map = object(&value, "content_block_delta")?;
        allowed(map, &["type", "index", "delta"], "content_block_delta")?;
        if required_string(map, "type", "content_block_delta")? != "content_block_delta" {
            return Err(TransformError("content_block_delta.type 无效".to_string()));
        }
        let index = required_index(map, "content_block_delta")?;
        let delta = object(
            map.get("delta")
                .ok_or_else(|| TransformError("content_block_delta 缺少 delta".to_string()))?,
            "content_block_delta.delta",
        )?;
        let kind = required_string(delta, "type", "content_block_delta.delta")?;
        match (self.blocks.get_mut(&index), kind.as_str()) {
            (Some(Block::Text { text, stopped }), "text_delta") => {
                allowed(delta, &["type", "text"], "Anthropic text delta")?;
                if *stopped {
                    return Err(TransformError(
                        "文本 block 已结束后继续发送 delta".to_string(),
                    ));
                }
                let value = required_string(delta, "text", "Anthropic text delta")?;
                text.push_str(&value);
                if !value.is_empty() {
                    append_event(
                        output,
                        "response.output_text.delta",
                        json!({
                            "type": "response.output_text.delta",
                            "output_index": index,
                            "content_index": 0,
                            "delta": value,
                        }),
                    );
                }
            }
            (
                Some(Block::Tool {
                    arguments,
                    stopped,
                    kind,
                    ..
                }),
                "input_json_delta",
            ) => {
                allowed(delta, &["type", "partial_json"], "Anthropic tool delta")?;
                if *stopped {
                    return Err(TransformError(
                        "工具 block 已结束后继续发送 delta".to_string(),
                    ));
                }
                let value = required_string(delta, "partial_json", "Anthropic tool delta")?;
                arguments.push_str(&value);
                if !value.is_empty() && *kind == ToolKind::Function {
                    append_event(
                        output,
                        "response.function_call_arguments.delta",
                        json!({
                            "type": "response.function_call_arguments.delta",
                            "output_index": index,
                            "delta": value,
                        }),
                    );
                }
            }
            (Some(Block::Thinking { text, stopped, .. }), "thinking_delta") => {
                allowed(delta, &["type", "thinking"], "Anthropic thinking delta")?;
                if *stopped {
                    return Err(TransformError(
                        "推理 block 已结束后继续发送 delta".to_string(),
                    ));
                }
                text.push_str(&required_string(
                    delta,
                    "thinking",
                    "Anthropic thinking delta",
                )?);
            }
            (Some(Block::Thinking { signature, .. }), "signature_delta") => {
                allowed(delta, &["type", "signature"], "Anthropic signature delta")?;
                let value = required_string(delta, "signature", "Anthropic signature delta")?;
                signature.get_or_insert_with(String::new).push_str(&value);
            }
            (Some(_), _) => {
                return Err(TransformError(
                    "Anthropic SSE content block delta 与起始类型不匹配".to_string(),
                ))
            }
            (None, _) => {
                return Err(TransformError(
                    "content_block_delta 找不到起始 block".to_string(),
                ))
            }
        }
        Ok(())
    }

    pub(super) fn block_stop(
        &mut self,
        frame: &Frame,
        output: &mut Vec<u8>,
    ) -> Result<(), TransformError> {
        let value = json_data(frame, "content_block_stop")?;
        let map = object(&value, "content_block_stop")?;
        allowed(map, &["type", "index"], "content_block_stop")?;
        if required_string(map, "type", "content_block_stop")? != "content_block_stop" {
            return Err(TransformError("content_block_stop.type 无效".to_string()));
        }
        let index = required_index(map, "content_block_stop")?;
        let id = self.id.clone();
        match self.blocks.get_mut(&index) {
            Some(Block::Thinking {
                text,
                signature,
                stopped,
                reasoning: slot,
            }) if !*stopped => {
                *stopped = true;
                if !text.is_empty() {
                    let transport = self.reasoning_transport.as_ref().ok_or_else(|| {
                        TransformError("当前转换缺少本机推理续接通道".to_string())
                    })?;
                    let sealed = transport
                        .from_anthropic_thinking(std::mem::take(text), signature.take())?;
                    release_reasoning(output, id.as_deref(), index, &sealed);
                    *slot = Some(sealed);
                }
                Ok(())
            }
            Some(Block::Redacted {
                data,
                stopped,
                reasoning: slot,
            }) if !*stopped => {
                *stopped = true;
                let transport = self
                    .reasoning_transport
                    .as_ref()
                    .ok_or_else(|| TransformError("当前转换缺少本机推理续接通道".to_string()))?;
                let sealed = transport.from_redacted(data.clone())?;
                release_reasoning(output, id.as_deref(), index, &sealed);
                *slot = Some(sealed);
                Ok(())
            }
            Some(
                Block::Text { stopped, .. }
                | Block::Thinking { stopped, .. }
                | Block::Redacted { stopped, .. }
                | Block::Tool { stopped, .. },
            ) if !*stopped => {
                *stopped = true;
                Ok(())
            }
            Some(_) => Err(TransformError(
                "Anthropic SSE 重复 content_block_stop".to_string(),
            )),
            None => Err(TransformError(
                "content_block_stop 找不到起始 block".to_string(),
            )),
        }
    }
}
