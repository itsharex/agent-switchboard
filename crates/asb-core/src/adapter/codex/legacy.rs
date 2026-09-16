//! Explicit migration for retired Codex provider contracts.
//!
//! Older configurations routed third parties through a dedicated
//! `model_providers` table (managed ids or user tables) instead of the
//! builtin `openai` provider with `openai_base_url`. Rewriting that shape is
//! the only migration path: parse first, produce a typed preview, and touch
//! nothing else. Authentication files are never read or written here, and a
//! legacy shape whose routing endpoint cannot be determined fails closed
//! instead of silently degrading to the official login.

use toml_edit::{DocumentMut, Item, TableLike};

use crate::contracts::CodexUpstream;

/// Provider ids this application once managed through dedicated
/// `model_providers` tables. They are ASB-owned legacy artifacts, never user
/// data; every other table belongs to the user and is preserved.
pub const RETIRED_CODEX_PROVIDER_IDS: [&str; 3] = ["openai", "agent_switchboard", "OpenAi"];

/// One legacy-configuration migration: the rewritten text plus every fact a
/// caller must surface, so no credential or route detail is lost silently.
#[derive(Debug, Clone, PartialEq)]
pub struct CodexLegacyMigration {
    /// Configuration rewritten to the current builtin-`openai` contract.
    pub normalized: String,
    /// The selected legacy provider id (rewritten to `openai` inside
    /// `normalized`).
    pub provider_id: String,
    /// Declared display `name` of the selected table; `None` is the
    /// name-less legacy shape.
    pub provider_name: Option<String>,
    /// Routing endpoint carried into `openai_base_url`.
    pub endpoint: String,
    /// `wire_api` dialect declared by the selected table, when recognized.
    pub upstream: Option<&'static str>,
    /// ASB-owned tables removed by the migration.
    pub removed_tables: Vec<String>,
    /// Unrelated tables kept untouched (the CLI can still select them).
    pub kept_idle_tables: Vec<String>,
    /// Notes for credentials the rewrite cannot carry; values are never
    /// echoed.
    pub credential_hints: Vec<String>,
    pub warnings: Vec<String>,
}

impl CodexLegacyMigration {
    /// Single-line summary for runtime logs.
    pub fn summary(&self) -> String {
        format!(
            "provider {} → openai，端点 {}，移除表 {:?}，保留闲置表 {:?}，警告 {} 条",
            self.provider_id,
            self.endpoint,
            self.removed_tables,
            self.kept_idle_tables,
            self.warnings.len()
        )
    }
}

/// Parses a legacy configuration and rewrites it to the current contract.
/// `Ok(None)` means the text already uses the current contract and must be
/// written back unchanged.
pub fn normalize_legacy_codex_configuration(
    configuration: &str,
) -> Result<Option<CodexLegacyMigration>, String> {
    let mut document = configuration
        .parse::<DocumentMut>()
        .map_err(|_| "Codex 配置无法解析".to_string())?;
    if let Some(item) = document.get("model_provider") {
        if item.as_str().is_none() {
            return Err("Codex model_provider 不是有效字符串，已拒绝迁移；请重新应用供应商".into());
        }
    }
    let selected = string_value(document.get("model_provider")).unwrap_or_else(|| "openai".into());
    let selected_is_builtin = selected == "openai";
    let providers = document
        .get("model_providers")
        .and_then(Item::as_table_like);
    let retired_present: Vec<String> = providers
        .map(|table| {
            RETIRED_CODEX_PROVIDER_IDS
                .into_iter()
                .filter(|id| table.get(*id).is_some())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let selected_table = providers.and_then(|table| table.get(selected.as_str()));
    if selected_is_builtin && retired_present.is_empty() {
        return Ok(None);
    }

    // Read every fact from the legacy text before rewriting anything. The
    // live `openai_base_url` is the actual builtin routing fact and wins over
    // stale table data.
    let table = selected_table.and_then(Item::as_table_like);
    let live_endpoint = string_value(document.get("openai_base_url"));
    if table.is_none() && !selected_is_builtin && live_endpoint.is_none() {
        return Err(format!(
            "已选 model_provider = {selected} 没有对应供应商表，已拒绝迁移；请重新应用供应商"
        ));
    }
    let wire_api = table.and_then(|value| table_string(value, "wire_api"));
    let (upstream, upstream_label, upstream_recognized) = resolve_upstream(wire_api);
    let provider_name = table
        .and_then(|value| table_string(value, "name"))
        .map(str::to_string);
    let raw_endpoint = live_endpoint
        .or_else(|| {
            table
                .and_then(|value| table_string(value, "base_url"))
                .map(str::to_string)
        })
        .ok_or_else(|| {
            "旧 Codex provider 契约缺少可迁移的服务地址，已拒绝静默转为官方登录；请重新应用供应商"
                .to_string()
        })?;
    let endpoint = normalize_migration_endpoint(&raw_endpoint, upstream)?;

    let mut credential_hints = Vec::new();
    for id in &retired_present {
        if let Some(removed) = providers
            .and_then(|table| table.get(id.as_str()))
            .and_then(Item::as_table_like)
        {
            collect_credential_hints(id, removed, &mut credential_hints);
        }
    }
    let mut warnings = Vec::new();
    if let (Some(value), false) = (wire_api, upstream_recognized) {
        warnings.push(format!(
            "旧供应商表的 wire_api = {value} 不受支持，请在应用供应商时重新选择上游协议"
        ));
    }
    if let Some(value) = table.and_then(|value| value.get("experimental_bearer_token")) {
        if !selected_is_builtin && value.as_str().is_some() {
            warnings.push(
                "选中的旧供应商表将退为闲置；其内嵌凭据不再生效，密钥改由本应用的账号体系提供"
                    .into(),
            );
        }
    }

    // Rewrite only the ASB-owned routing facts.
    document["model_provider"] = toml_edit::value("openai");
    document["openai_base_url"] = toml_edit::value(endpoint.clone());
    let mut kept_idle_tables = Vec::new();
    if let Some(providers) = document
        .get_mut("model_providers")
        .and_then(Item::as_table_like_mut)
    {
        for id in &retired_present {
            providers.remove(id);
        }
        for (key, _) in providers.iter() {
            kept_idle_tables.push(key.to_string());
        }
        if providers.is_empty() {
            document.as_table_mut().remove("model_providers");
        }
    }

    Ok(Some(CodexLegacyMigration {
        normalized: document.to_string(),
        provider_id: selected,
        provider_name,
        endpoint,
        upstream: upstream_label,
        removed_tables: retired_present,
        kept_idle_tables,
        credential_hints,
        warnings,
    }))
}

/// Mirrors the discovery mapping; `recognized` is false only for a wire_api
/// that is present but unknown.
fn resolve_upstream(wire_api: Option<&str>) -> (CodexUpstream, Option<&'static str>, bool) {
    match wire_api {
        None | Some("responses") => (CodexUpstream::Responses, Some("responses"), true),
        Some("chat") | Some("chat_completions") => {
            (CodexUpstream::ChatCompletions, Some("chat"), true)
        }
        Some("anthropic") | Some("anthropic_messages") => {
            (CodexUpstream::AnthropicMessages, Some("anthropic"), true)
        }
        Some(_) => (CodexUpstream::Responses, None, false),
    }
}

fn normalize_migration_endpoint(source: &str, upstream: CodexUpstream) -> Result<String, String> {
    let mut url = url::Url::parse(source).map_err(|_| "旧 Codex 契约的服务地址无效".to_string())?;
    if matches!(
        upstream,
        CodexUpstream::Responses | CodexUpstream::ChatCompletions
    ) && url.path().trim_matches('/').is_empty()
    {
        url.set_path("/v1");
    }
    crate::endpoint::validate_base_url(url.as_str(), upstream.protocol())
        .map_err(|_| "旧 Codex 契约的服务地址无效".to_string())?;
    Ok(url.to_string().trim_end_matches('/').to_string())
}

fn collect_credential_hints(id: &str, table: &dyn TableLike, hints: &mut Vec<String>) {
    if table.get("experimental_bearer_token").is_some() {
        hints.push(format!(
            "已移除表 {id} 内嵌的 experimental_bearer_token；其值不会出现在迁移结果中，请在应用供应商时重新补全密钥"
        ));
    }
    if let Some(env_key) = table_string(table, "env_key") {
        hints.push(format!(
            "已移除表 {id} 声明 env_key = {env_key}；该密钥由环境变量提供，迁移不会读取或迁移它"
        ));
    }
}

fn string_value(item: Option<&Item>) -> Option<String> {
    item.and_then(Item::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn table_string<'a>(table: &'a dyn TableLike, key: &str) -> Option<&'a str> {
    table
        .get(key)
        .and_then(Item::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
