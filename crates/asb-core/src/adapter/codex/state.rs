use toml_edit::{DocumentMut, Item, TableLike};

use crate::adapter::AdapterError;
use crate::contracts::{CodexModelSettings, KeyChange, RouteMode, RouteState, SwitchPlan};
use crate::ownership::{is_owned, CODEX_PROVIDER_ID};
use crate::AppKind;

use crate::adapter::codex::document::{item_at, item_repr, parse};

/// Codex's built-in provider id.
pub const OFFICIAL_PROVIDER: &str = CODEX_PROVIDER_ID;

pub(crate) fn matches_provider_settings(
    text: &str,
    plan: &SwitchPlan,
) -> Result<bool, AdapterError> {
    let doc = parse(text)?;
    let provider = item_at(&doc, "model_provider")
        .and_then(item_repr)
        .unwrap_or_else(|| OFFICIAL_PROVIDER.to_string());
    Ok(provider == CODEX_PROVIDER_ID
        && (plan.profile.route_mode == RouteMode::Official
            || plan.is_gateway()
            || (plan.profile.upstream_protocol
                == Some(crate::contracts::UpstreamProtocol::Responses)
                && plan.client_base_url().is_some())))
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
            // `agents.max_threads` is not a current setting and never enters
            // the editor contract. It remains diff-owned solely so its one-way
            // cleanup is explicit in every candidate that removes it.
            if is_owned(AppKind::Codex, &path) || path == "agents.max_threads" {
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
    let builtin_openai = provider_id == OFFICIAL_PROVIDER;
    let provider_table = (!builtin_openai)
        .then(|| doc.get("model_providers").and_then(Item::as_table_like))
        .flatten()
        .and_then(|providers| providers.get(&provider_id))
        .and_then(Item::as_table_like);
    let declared_name = provider_table
        .and_then(|table| table.get("name"))
        .and_then(item_repr)
        .filter(|value| !value.trim().is_empty());
    let base_url = if builtin_openai {
        get("openai_base_url")
    } else {
        provider_table
            .and_then(|table| table.get("base_url"))
            .and_then(item_repr)
    }
    .filter(|value| !value.trim().is_empty());
    let wire_api = provider_table
        .and_then(|table| table.get("wire_api"))
        .and_then(item_repr)
        .filter(|value| !value.trim().is_empty());
    let custom = !builtin_openai || base_url.is_some();
    RouteState {
        app: AppKind::Codex,
        route_mode: if custom {
            RouteMode::Custom
        } else {
            RouteMode::Official
        },
        provider_name: declared_name.or(Some(provider_id)),
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


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retired_max_threads_appears_as_an_explicit_removal_in_the_diff() {
        let changes = owned_diff("", "[agents]\nmax_threads = 4\n").expect("diff");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].key, "agents.max_threads");
        assert_eq!(changes[0].after, None);
    }
}
