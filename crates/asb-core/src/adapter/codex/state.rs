use toml_edit::{DocumentMut, Item, TableLike};

use crate::adapter::AdapterError;
use crate::contracts::{CodexModelSettings, KeyChange, RouteMode, RouteState};
use crate::ownership::is_owned;
use crate::AppKind;

use crate::adapter::codex::document::{item_at, item_repr, parse};

/// Codex's built-in provider id.
pub const OFFICIAL_PROVIDER: &str = "openai";

pub(crate) fn uses_builtin_provider(text: &str) -> Result<bool, AdapterError> {
    let doc = parse(text)?;
    Ok(item_at(&doc, "model_provider")
        .and_then(item_repr)
        .is_none_or(|provider| provider == OFFICIAL_PROVIDER))
}

/// Collects every owned scalar path and its textual value.
fn collect_owned_scalars(
    table: &dyn TableLike,
    prefix: &str,
    out: &mut std::collections::BTreeMap<String, String>,
) {
    for (key, item) in table.iter() {
        let path = if prefix.is_empty() {
            key.to_string()
        } else {
            format!("{prefix}.{key}")
        };
        if let Some(value) = item_repr(item) {
            if is_owned(AppKind::Codex, &path) {
                out.insert(path.clone(), value);
            }
        }
        if let Some(sub) = item.as_table_like() {
            collect_owned_scalars(sub, &path, out);
        }
    }
}

/// Owned-key diff between the live text and a previous copy.
pub(crate) fn owned_diff(current: &str, previous: &str) -> Result<Vec<KeyChange>, AdapterError> {
    let mut current_values = std::collections::BTreeMap::new();
    collect_owned_scalars(parse(current)?.as_table(), "", &mut current_values);
    let mut previous_values = std::collections::BTreeMap::new();
    collect_owned_scalars(parse(previous)?.as_table(), "", &mut previous_values);
    Ok(crate::adapter::diff_owned_maps(
        &current_values,
        &previous_values,
    ))
}

/// Scope-of-effect warnings for facts inside this file that a `--profile`
/// launch or other override can shadow.
fn scope_warnings(doc: &DocumentMut) -> Vec<String> {
    let profiles = doc
        .as_table()
        .get("profiles")
        .and_then(Item::as_table_like)
        .map(|table| table.len())
        .unwrap_or(0);
    if profiles > 0 {
        vec![format!(
            "config.toml 定义了 {profiles} 个配置档；使用 --profile 启动 Codex 时会覆盖这里的用户级设置"
        )]
    } else {
        vec![]
    }
}

/// Reads the active routing facts from Codex configuration text.
pub fn route_state(text: &str) -> RouteState {
    let doc = parse(text).expect("caller validates syntax first");
    let get = |path: &str| item_at(&doc, path).and_then(item_repr);
    let provider_id = get("model_provider").unwrap_or_else(|| OFFICIAL_PROVIDER.to_string());
    let custom_provider = provider_id != OFFICIAL_PROVIDER;
    let base_url = if custom_provider {
        get(&format!("model_providers.{provider_id}.base_url"))
    } else {
        get("openai_base_url")
    };
    let custom = custom_provider || base_url.is_some();
    let wire_api = custom_provider
        .then(|| get(&format!("model_providers.{provider_id}.wire_api")))
        .flatten();
    RouteState {
        app: AppKind::Codex,
        route_mode: if custom {
            RouteMode::Custom
        } else {
            RouteMode::Official
        },
        provider_name: custom_provider
            .then(|| get(&format!("model_providers.{provider_id}.name")))
            .flatten(),
        model: get("model"),
        base_url,
        wire_api,
        codex_model_options: Some(CodexModelSettings {
            context_window: item_at(&doc, "model_context_window")
                .and_then(|item| item.as_value())
                .and_then(|value| value.as_integer())
                .and_then(|value| u64::try_from(value).ok()),
        }),
        haiku_model: None,
        sonnet_model: None,
        opus_model: None,
        available_models: None,
        scope_warnings: scope_warnings(&doc),
    }
}
