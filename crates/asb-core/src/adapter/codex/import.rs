use crate::contracts::{ResponsesOptions, ResponsesRequestMode};
use toml_edit::{Item, TableLike};

/// Imported external Responses profiles use the standard request contract.
pub(crate) fn import_responses_options(table: &dyn TableLike) -> Result<ResponsesOptions, String> {
    if table.get("supports_websockets").is_some() {
        return Err("不再导入供应商 WebSocket 设置；第三方统一通过网关 HTTP/SSE".into());
    }
    if table
        .get("requires_openai_auth")
        .is_some_and(|value| Item::as_bool(value).is_none())
    {
        return Err("requires_openai_auth 必须为布尔值".into());
    }
    Ok(ResponsesOptions {
        request_mode: ResponsesRequestMode::Standard,
    })
}
