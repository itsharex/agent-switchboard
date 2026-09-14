use crate::contracts::{CodexPromptCacheRouting, CodexRequestOptions};
use serde_json::{Map,Value};
pub(super) fn parse(meta:&Map<String,Value>,warnings:&mut Vec<String>) -> Result<Option<CodexRequestOptions>,String> {
    let mode=meta.get("promptCacheRouting").map(|value|serde_json::from_value::<CodexPromptCacheRouting>(value.clone())
        .map_err(|_|"meta.promptCacheRouting 必须是 auto/enabled/disabled".to_string())).transpose()?;
    let emulate=meta.get("impersonateClaudeCode").map(|value|value.as_bool()
        .ok_or_else(||"meta.impersonateClaudeCode 必须是布尔值".to_string())).transpose()?;
    if mode.is_none() && emulate.is_none() { return Ok(None); }
    warnings.retain(|warning| !matches!(warning.as_str(),"未导入: meta.promptCacheRouting"|"未导入: meta.impersonateClaudeCode"));
    Ok(Some(CodexRequestOptions { prompt_cache_routing:mode.unwrap_or_default(),
        emulate_claude_code:emulate.unwrap_or(false), ..Default::default() }))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_codex_options_without_removing_unrelated_warnings() {
        let meta=serde_json::json!({"promptCacheRouting":"enabled","impersonateClaudeCode":false});
        let mut warnings=vec!["未导入: meta.promptCacheRouting".into(),"未导入: meta.impersonateClaudeCode".into(),"未导入: meta.other".into()];
        let options=parse(meta.as_object().unwrap(),&mut warnings).unwrap().unwrap();
        assert_eq!(options.prompt_cache_routing,CodexPromptCacheRouting::Enabled);
        assert_eq!(warnings,["未导入: meta.other"]);
        assert!(parse(serde_json::json!({"impersonateClaudeCode":"true"}).as_object().unwrap(),&mut warnings).is_err());
    }
}
