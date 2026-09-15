//! Request behavior that belongs only to Codex protocol bridges, not Claude profiles.
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CodexPromptCacheRouting {
    #[default]
    Auto,
    Enabled,
    Disabled,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodexAnthropicCacheTtl {
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "1h")]
    OneHour,
}
impl CodexAnthropicCacheTtl {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FiveMinutes => "5m",
            Self::OneHour => "1h",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodexRequestOptions {
    #[serde(default)]
    pub prompt_cache_routing: CodexPromptCacheRouting,
    #[serde(default)]
    pub anthropic_cache_ttl: Option<CodexAnthropicCacheTtl>,
    #[serde(default)]
    pub emulate_claude_code: bool,
}
impl CodexRequestOptions {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
    pub fn validate(&self, upstream: super::CodexUpstream) -> Result<(), String> {
        if self.prompt_cache_routing != CodexPromptCacheRouting::Auto
            && upstream != super::CodexUpstream::ChatCompletions
        {
            return Err("Codex prompt cache routing 仅用于 Chat Completions 上游".into());
        }
        if (self.anthropic_cache_ttl.is_some() || self.emulate_claude_code)
            && upstream != super::CodexUpstream::AnthropicMessages
        {
            return Err("Codex Anthropic 缓存与客户端兼容选项仅用于 Anthropic 上游".into());
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn options_are_strict_and_cannot_silently_cross_protocols() {
        assert!(
            serde_json::from_str::<CodexRequestOptions>(r#"{"promptCacheRouting":"maybe"}"#)
                .is_err()
        );
        assert!(
            serde_json::from_str::<CodexRequestOptions>(r#"{"anthropicCacheTtl":"2h"}"#).is_err()
        );
        assert!(serde_json::from_str::<CodexRequestOptions>(r#"{"unknown":true}"#).is_err());
        let chat = CodexRequestOptions {
            prompt_cache_routing: CodexPromptCacheRouting::Enabled,
            ..Default::default()
        };
        assert!(chat
            .validate(super::super::CodexUpstream::ChatCompletions)
            .is_ok());
        assert!(chat
            .validate(super::super::CodexUpstream::Responses)
            .is_err());
        let anthropic = CodexRequestOptions {
            anthropic_cache_ttl: Some(CodexAnthropicCacheTtl::OneHour),
            emulate_claude_code: true,
            ..Default::default()
        };
        assert!(anthropic
            .validate(super::super::CodexUpstream::AnthropicMessages)
            .is_ok());
        assert!(anthropic
            .validate(super::super::CodexUpstream::ChatCompletions)
            .is_err());
    }
}
