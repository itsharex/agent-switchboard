mod catalog;
mod completion;
mod query;
mod reasoning;
mod request_options;
use catalog::catalog_seeds;

use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item, TableLike};
use url::Url;

use crate::ccswitch::row::{CcSwitchProposal, CcSwitchProviderDraft, CcSwitchRow, CodexImportSeed};
use crate::ccswitch::usage::map_usage_query;
use crate::contracts::{
    codex_official_draft, AppKind, AuthenticationScheme, CodexEndpoint, CodexUpstream,
    ProviderConnectionOptions, ResponsesRequestMode,
};

const SOURCE_TABLE_KEYS: [&str; 7] = [
    "name",
    "base_url",
    "wire_api",
    "requires_openai_auth",
    "experimental_bearer_token",
    "supports_websockets",
    "query_params",
];

/// Maps one current imported Codex row into an importable proposal: a
/// third-party row becomes a completion seed the backend resolves into the
/// strict profile store; an official-login row (no custom endpoint) becomes
/// the generic store's official record.
pub(crate) fn map_codex(key: String, row: &CcSwitchRow) -> Result<CcSwitchProposal, String> {
    let (settings, mut warnings) = parse_settings(&row.settings_config)?;
    let document = parse_document(&settings.config)?;
    let parameters = crate::adapter::read_provider_parameters(AppKind::Codex, &settings.config)
        .map_err(|error| error.to_string())?;
    if official_route(&document) {
        return Ok(CcSwitchProposal {
            key,
            draft: CcSwitchProviderDraft::CodexOfficial(codex_official_draft(parameters)),
            warnings,
        });
    }
    let (route, route_warnings) = selected_route(&document)?;
    warnings.extend(route_warnings);
    let (mut metadata, meta_warnings) = SourceMetadata::parse(row.meta.as_deref())?;
    warnings.extend(meta_warnings);
    let upstream = metadata.upstream(route.upstream)?;
    let endpoint = normalize_endpoint(&route.base_url, upstream, metadata.connection.is_full_url)?;
    let endpoint = query::apply(&document, endpoint, upstream, &mut metadata.connection)?;
    let (api_key, key_from_auth) = source_api_key(&settings.auth, &route, &document);
    if api_key.is_empty() {
        warnings.push("来源未提供可用 API 密钥；导入前请补全".to_string());
    }
    warn_ignored_auth(&settings.auth, key_from_auth, &mut warnings);
    let default_model = required_top_level_string(&document, "model", "主模型")?;
    let usage_query = map_usage_query(row.meta.as_deref(), &mut warnings);
    let catalog = catalog_seeds(settings.catalog.as_ref(), &default_model, &mut warnings);
    let seed = CodexImportSeed {
        name: row.name.clone(),
        endpoint,
        api_key,
        authentication: metadata.authentication(upstream),
        connection: metadata.connection,
        upstream,
        request_mode: ResponsesRequestMode::Standard,
        chat_reasoning: metadata.chat_reasoning,
        default_model,
        catalog,
        parameters,
        notes: row.notes.clone(),
        website_url: row.website_url.clone(),
        usage_query,
        warnings: warnings.clone(),
    };

    Ok(CcSwitchProposal {
        key,
        draft: CcSwitchProviderDraft::Codex(seed),
        warnings,
    })
}

/// Whether the row routes to Codex's own login instead of a custom endpoint.
/// Official credentials stay client-owned; only the selectable route is
/// imported, exactly like a Claude official row.
fn official_route(document: &DocumentMut) -> bool {
    let provider = document
        .get("model_provider")
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("openai");
    provider == "openai" && item_string(document.get("openai_base_url")).is_none()
}

struct SourceSettings {
    auth: Map<String, Value>,
    config: String,
    catalog: Option<Value>,
}

fn parse_settings(text: &str) -> Result<(SourceSettings, Vec<String>), String> {
    let root: Value = serde_json::from_str(text).map_err(|_| "配置无法解析".to_string())?;
    let object = root
        .as_object()
        .ok_or_else(|| "Codex 供应商配置必须是对象".to_string())?;
    let config = object
        .get("config")
        .and_then(Value::as_str)
        .ok_or_else(|| "供应商 config 必须是 TOML 文本".to_string())?
        .to_string();
    let auth = match object.get("auth") {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(auth)) => auth.clone(),
        Some(_) => return Err("供应商 auth 必须是对象".to_string()),
    };
    let catalog = object
        .get("modelCatalog")
        .filter(|value| !value.is_null())
        .cloned();
    let mut warnings = Vec::new();
    for key in object.keys() {
        if !matches!(key.as_str(), "config" | "auth" | "modelCatalog") {
            warnings.push(format!("未导入: settings_config.{key}"));
        }
    }
    Ok((
        SourceSettings {
            auth,
            config,
            catalog,
        },
        warnings,
    ))
}

fn parse_document(text: &str) -> Result<DocumentMut, String> {
    text.parse::<DocumentMut>()
        .map_err(|_| "供应商 TOML 无法解析".to_string())
}

struct SourceRoute {
    base_url: String,
    upstream: CodexUpstream,
    bearer_token: Option<String>,
}

fn selected_route(document: &DocumentMut) -> Result<(SourceRoute, Vec<String>), String> {
    let provider = document
        .get("model_provider")
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("openai");
    if provider == "openai" {
        let base_url = document
            .get("openai_base_url")
            .and_then(Item::as_str)
            .map(str::to_string)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "缺少 openai_base_url".to_string())?;
        return Ok((
            SourceRoute {
                base_url,
                upstream: CodexUpstream::Responses,
                bearer_token: item_string(document.get("experimental_bearer_token")),
            },
            Vec::new(),
        ));
    }

    let providers = document
        .get("model_providers")
        .and_then(Item::as_table_like)
        .ok_or_else(|| "缺少 model_providers 表".to_string())?;
    let table = providers
        .get(provider)
        .and_then(Item::as_table_like)
        .ok_or_else(|| format!("已选 model_provider = {provider} 没有对应表"))?;
    let mut warnings = Vec::new();
    for (key, _) in table.iter() {
        if !SOURCE_TABLE_KEYS.contains(&key) {
            warnings.push(format!("未导入: model_providers.{provider}.{key}"));
        }
    }
    let base_url = required_table_string(table, "base_url", "已选供应商表的 base_url")?;
    let wire_api = table_string(table, "wire_api").unwrap_or("responses");
    let upstream = match wire_api {
        "responses" => CodexUpstream::Responses,
        "chat" => CodexUpstream::ChatCompletions,
        "anthropic" => CodexUpstream::AnthropicMessages,
        _ => return Err("已选供应商表的 wire_api 不受支持".to_string()),
    };
    Ok((
        SourceRoute {
            base_url,
            upstream,
            bearer_token: table_string(table, "experimental_bearer_token").map(str::to_string),
        },
        warnings,
    ))
}

fn table_string<'a>(table: &'a dyn TableLike, name: &str) -> Option<&'a str> {
    table
        .get(name)
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn required_table_string(table: &dyn TableLike, name: &str, label: &str) -> Result<String, String> {
    table_string(table, name)
        .map(str::to_string)
        .ok_or_else(|| format!("缺少 {label}"))
}

fn item_string(item: Option<&Item>) -> Option<String> {
    item.and_then(Item::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn required_top_level_string(
    document: &DocumentMut,
    key: &str,
    label: &str,
) -> Result<String, String> {
    item_string(document.get(key)).ok_or_else(|| format!("缺少 {label}"))
}

struct SourceMetadata {
    chat_reasoning: crate::contracts::CodexChatReasoning,
    api_format: Option<String>,
    connection: ProviderConnectionOptions,
}

impl SourceMetadata {
    fn parse(text: Option<&str>) -> Result<(Self, Vec<String>), String> {
        let Some(text) = text.filter(|text| !text.trim().is_empty()) else {
            return Ok((
                Self {
                    api_format: None,
                    chat_reasoning: crate::contracts::CodexChatReasoning::Unsupported,
                    connection: ProviderConnectionOptions::default(),
                },
                Vec::new(),
            ));
        };
        let mut warnings = Vec::new();
        let root = crate::ccswitch::mapping::parse_meta(Some(text), &mut warnings)?;
        let object = root
            .as_object()
            .ok_or_else(|| "meta 必须是对象".to_string())?;
        let api_format = match optional_meta_string(object, "apiFormat")? {
            Some(value) => Some(value),
            None => optional_meta_string(object, "api_format")?,
        };
        let mut connection = crate::ccswitch::mapping::map_connection(&root, &mut warnings);
        connection.codex = request_options::parse(object, &mut warnings)?;
        let chat_reasoning = reasoning::parse(object.get("codexChatReasoning"))?;
        warnings.retain(|warning| warning != "未导入: meta.codexChatReasoning");
        Ok((
            Self {
                api_format,
                connection,
                chat_reasoning,
            },
            warnings,
        ))
    }

    fn upstream(&self, fallback: CodexUpstream) -> Result<CodexUpstream, String> {
        match self.api_format.as_deref() {
            None => Ok(fallback),
            Some("openai_responses") | Some("responses") => Ok(CodexUpstream::Responses),
            Some("openai_chat") | Some("chat") | Some("chat_completions") => {
                Ok(CodexUpstream::ChatCompletions)
            }
            Some("anthropic") | Some("anthropic_messages") => Ok(CodexUpstream::AnthropicMessages),
            Some(_) => Err("meta.apiFormat 不受支持".to_string()),
        }
    }

    fn authentication(&self, upstream: CodexUpstream) -> Option<AuthenticationScheme> {
        match self.connection.api_key_field {
            Some(crate::contracts::ClaudeApiKeyField::AnthropicAuthToken) => {
                Some(AuthenticationScheme::Bearer)
            }
            Some(crate::contracts::ClaudeApiKeyField::AnthropicApiKey) => {
                Some(AuthenticationScheme::XApiKey)
            }
            None => (upstream == CodexUpstream::AnthropicMessages)
                .then_some(AuthenticationScheme::XApiKey),
        }
    }
}

fn optional_meta_string(object: &Map<String, Value>, key: &str) -> Result<Option<String>, String> {
    object
        .get(key)
        .map(|value| {
            value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("meta.{key} 必须是非空字符串"))
        })
        .transpose()
}

/// Resolves the source credential without rejecting the row: a missing key
/// becomes an empty seed value plus a warning, and the editor enforces
/// completion before saving.
fn source_api_key(
    auth: &Map<String, Value>,
    route: &SourceRoute,
    document: &DocumentMut,
) -> (String, bool) {
    if let Some(value) = route.bearer_token.clone() {
        return (value, false);
    }
    if let Some(value) = item_string(document.get("experimental_bearer_token")) {
        return (value, false);
    }
    match auth
        .get("OPENAI_API_KEY")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty() && *key != "PROXY_MANAGED")
    {
        Some(value) => (value.to_string(), true),
        None => (String::new(), false),
    }
}

fn has_oauth_residue(auth: &Map<String, Value>) -> bool {
    auth.get("tokens").is_some_and(|value| !value.is_null())
}

fn warn_ignored_auth(
    auth: &Map<String, Value>,
    imported_from_auth: bool,
    warnings: &mut Vec<String>,
) {
    if has_oauth_residue(auth) {
        warnings.push("未导入: auth.tokens".to_string());
    }
    for key in auth.keys() {
        if key == "OPENAI_API_KEY" && imported_from_auth {
            continue;
        }
        if key != "tokens" {
            warnings.push(format!("未导入: auth.{key}"));
        }
    }
}

fn normalize_endpoint(
    source: &str,
    upstream: CodexUpstream,
    is_full_url: bool,
) -> Result<CodexEndpoint, String> {
    if source.trim() != source {
        return Err("Codex 服务地址无效".to_string());
    }
    let mut url = Url::parse(source).map_err(|_| "Codex 服务地址无效".to_string())?;
    if is_full_url {
        crate::endpoint::validate_full_url(source).map_err(|_| "Codex 服务地址无效".to_string())?;
        return Ok(CodexEndpoint(url.to_string()));
    }
    if matches!(
        upstream,
        CodexUpstream::Responses | CodexUpstream::ChatCompletions
    ) && url.path().trim_matches('/').is_empty()
    {
        url.set_path("/v1");
    } else if url.path() != "/" {
        let path = url.path().trim_end_matches('/').to_string();
        url.set_path(&path);
    }
    let endpoint = CodexEndpoint(url.to_string());
    crate::endpoint::validate_base_url(&endpoint.0, upstream.protocol())
        .map_err(|_| "Codex 服务地址无效".to_string())?;
    Ok(endpoint)
}
