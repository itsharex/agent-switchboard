use crate::contracts::{ResponsesOptions, ResponsesRequestMode, RouteState, UpstreamProtocol};
use toml_edit::{DocumentMut, Item};

/// Resolves only configuration facts that an imported provider can retain.
/// Credential values remain in the separately read API-key cache.
pub(super) fn import_route(
    text: &str,
    route: &RouteState,
) -> Result<(UpstreamProtocol, Option<ResponsesOptions>), String> {
    let protocol = match route.wire_api.as_deref() {
        None | Some("responses") => UpstreamProtocol::Responses,
        Some("chat") | Some("chat_completions") => UpstreamProtocol::ChatCompletions,
        Some("anthropic") | Some("anthropic_messages") => {
            return Err(
                "Codex 转到 Anthropic Messages 必须明确设置最大输出 token，不能直接导入".into(),
            );
        }
        Some(_) => return Err("当前 Codex 配置的 wire_api 不受支持".into()),
    };
    let base_url = route
        .base_url
        .as_deref()
        .ok_or("当前 Codex 配置缺少服务地址")?;
    crate::endpoint::validate_base_url(base_url, protocol)?;
    let doc = text
        .parse::<DocumentMut>()
        .map_err(|_| "Codex 配置不是有效 TOML")?;
    let provider = doc
        .get("model_provider")
        .and_then(Item::as_str)
        .unwrap_or(crate::adapter::codex::OFFICIAL_PROVIDER);
    if provider == crate::adapter::codex::OFFICIAL_PROVIDER {
        return Ok((
            protocol,
            Some(ResponsesOptions {
                request_mode: ResponsesRequestMode::Standard,
            }),
        ));
    }
    let table = doc
        .get("model_providers")
        .and_then(|providers| providers.get(provider))
        .and_then(Item::as_table_like)
        .ok_or("当前 Codex 配置缺少选中的供应商表")?;
    if table.get("requires_openai_auth").and_then(Item::as_bool) != Some(true)
        || table.get("env_key").is_some()
        || table.get("experimental_bearer_token").is_some()
    {
        return Err(
            "当前 Codex 供应商未明确使用 auth.json API-key 凭据，无法保留其认证方式".into(),
        );
    }
    let options = crate::adapter::codex::import_responses_options(table)?;
    Ok((
        protocol,
        (protocol == UpstreamProtocol::Responses).then_some(options),
    ))
}
