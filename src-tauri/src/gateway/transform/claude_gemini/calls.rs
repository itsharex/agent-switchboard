//! Gemini may omit tool IDs, so response order must follow the original calls.
use super::*;
use std::collections::BTreeMap;
pub(super) struct Call {
    pub name: String,
    pub upstream_id: Option<String>,
    order: u64,
    resolved: bool,
}
#[derive(Default)]
pub(super) struct Calls {
    values: BTreeMap<String, Call>,
    next: u64,
}
impl Calls {
    pub fn remember(
        &mut self,
        id: &str,
        name: &str,
        upstream_id: Option<String>,
    ) -> Result<(), TransformError> {
        if self.values.get(id).is_some_and(|call| !call.resolved) {
            return error("Gemini 历史消息中存在重复的未完成工具调用");
        }
        self.values.insert(
            id.into(),
            Call {
                name: name.into(),
                upstream_id,
                order: self.next,
                resolved: false,
            },
        );
        self.next += 1;
        Ok(())
    }
    pub fn resolve(&mut self, id: &str) -> Result<&Call, TransformError> {
        let call = self
            .values
            .get_mut(id)
            .ok_or_else(|| TransformError("Gemini 工具结果找不到对应调用名称".into()))?;
        if call.resolved {
            return error("Gemini 同一工具调用不能返回重复结果");
        }
        call.resolved = true;
        Ok(call)
    }
    pub fn ordered<'a>(&self, parts: &'a [Part]) -> Vec<&'a Part> {
        let mut result = parts.iter().collect::<Vec<_>>();
        let mut start = 0;
        while start < result.len() {
            if !matches!(result[start], Part::ToolResult { .. }) {
                start += 1;
                continue;
            }
            let end = start
                + result[start..]
                    .iter()
                    .take_while(|part| matches!(part, Part::ToolResult { .. }))
                    .count();
            result[start..end].sort_by_key(|part| match part {
                Part::ToolResult { id, .. } => {
                    self.values.get(id).map_or(u64::MAX, |call| call.order)
                }
                _ => u64::MAX,
            });
            start = end;
        }
        result
    }
}
