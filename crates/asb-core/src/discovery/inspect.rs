use toml_edit::Item;

use crate::contracts::{AppKind, RouteMode, RouteState};

use crate::discovery::import::claude_import_model_fields;
use crate::discovery::report::{DiscoveredFile, DiscoveredState};

/// Inspects configuration content and classifies it. `text` is the raw file
/// content, already read by the caller.
pub fn inspect(app: AppKind, path: &str, text: Option<&str>) -> DiscoveredFile {
    let Some(text) = text else {
        return DiscoveredFile {
            app,
            path: path.to_string(),
            exists: false,
            state: DiscoveredState::Missing,
        };
    };

    if let Err(e) = crate::adapter::validate_syntax(app, text) {
        return DiscoveredFile {
            app,
            path: path.to_string(),
            exists: true,
            state: DiscoveredState::ParseError {
                message: e.message,
                line: e.line,
            },
        };
    }

    let route = crate::adapter::route_state(app, text);
    let (managed, mut warnings) = match app {
        AppKind::Codex => inspect_codex(text),
        AppKind::Claude => inspect_claude(text),
    };
    let claude_import_error = (app == AppKind::Claude)
        .then(|| claude_import_model_fields(&route).err())
        .flatten();
    let importable = if route.route_mode == RouteMode::Official {
        true
    } else {
        match app {
            AppKind::Codex => codex_import_is_supported(text, &route),
            AppKind::Claude => route.base_url.is_some() && claude_import_error.is_none(),
        }
    };
    if app == AppKind::Codex && route.route_mode == RouteMode::Custom && !importable {
        if matches!(
            route.wire_api.as_deref(),
            Some("anthropic") | Some("anthropic_messages")
        ) {
            warnings.push(
                "当前 Codex 配置使用 Anthropic Messages；新供应商必须明确设置最大输出 token，不能直接导入"
                    .to_string(),
            );
        } else {
            warnings.push("当前 Codex 配置无法作为供应商档案导入".to_string());
        }
    }
    if app == AppKind::Claude && route.route_mode == RouteMode::Custom {
        if let Some(error) = claude_import_error {
            warnings.push(format!("当前 Claude 配置无法作为供应商档案导入：{error}"));
        }
    }
    DiscoveredFile {
        app,
        path: path.to_string(),
        exists: true,
        state: DiscoveredState::Ok {
            route,
            managed,
            warnings,
            importable,
        },
    }
}

fn codex_import_is_supported(text: &str, route: &RouteState) -> bool {
    if route.route_mode != RouteMode::Custom || route.base_url.is_none() {
        return false;
    }
    if matches!(
        route.wire_api.as_deref(),
        Some("anthropic") | Some("anthropic_messages")
    ) {
        return false;
    }
    let Ok(doc) = text.parse::<toml_edit::DocumentMut>() else {
        return false;
    };
    let provider_id = doc
        .as_table()
        .get("model_provider")
        .and_then(Item::as_value)
        .and_then(|value| value.as_str());
    provider_id == Some(crate::adapter::codex::OFFICIAL_PROVIDER)
}

fn inspect_codex(text: &str) -> (bool, Vec<String>) {
    let parsed = text.parse::<toml_edit::DocumentMut>();
    let Ok(doc) = parsed else {
        return (false, vec![]); // caller already classified parse errors
    };
    let provider_id = doc
        .as_table()
        .get("model_provider")
        .and_then(Item::as_value)
        .and_then(|value| value.as_str())
        .unwrap_or(crate::adapter::codex::OFFICIAL_PROVIDER);
    let managed = provider_id == crate::adapter::codex::OFFICIAL_PROVIDER
        && doc
            .as_table()
            .get("openai_base_url")
            .and_then(Item::as_value)
            .and_then(|value| value.as_str())
            .is_some();

    (managed, vec![])
}

fn inspect_claude(text: &str) -> (bool, Vec<String>) {
    let Ok(root) = serde_json::from_str::<serde_json::Value>(text) else {
        return (false, vec![]);
    };
    let managed = root
        .pointer("/env/ANTHROPIC_BASE_URL")
        .and_then(|v| v.as_str())
        .is_some();
    let mut warnings = Vec::new();
    if root
        .pointer("/env/ANTHROPIC_AUTH_TOKEN")
        .and_then(|v| v.as_str())
        .is_some()
    {
        warnings.push("settings.json 的 env 中存在明文 ANTHROPIC_AUTH_TOKEN".to_string());
    }
    (managed, warnings)
}

pub(super) fn inspect_read(
    app: AppKind,
    path: &str,
    read: Result<Option<String>, String>,
) -> DiscoveredFile {
    match read {
        Ok(text) => inspect(app, path, text.as_deref()),
        Err(message) => DiscoveredFile {
            app,
            path: path.to_string(),
            exists: true,
            state: DiscoveredState::ReadError {
                message: crate::adapter::scrub_message(message),
            },
        },
    }
}
