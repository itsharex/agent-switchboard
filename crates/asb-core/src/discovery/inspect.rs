use toml_edit::Item;

use crate::contracts::{AppKind, RouteMode, RouteState};

use crate::claude_model::{import_models, ModelSource};
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
    let importable = import_supported(app, text, &route, &mut warnings);
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

fn import_supported(
    app: AppKind,
    text: &str,
    route: &RouteState,
    warnings: &mut Vec<String>,
) -> bool {
    if app == AppKind::Codex {
        if route.route_mode == RouteMode::Official {
            return true;
        }
        if route
            .base_url
            .as_deref()
            .is_some_and(crate::adapter::codex::is_gateway_base_url)
        {
            return false;
        }
        let mut importable = true;
        if route.base_url.is_none() {
            warnings.push("当前 Codex 自定义路由缺少服务地址".to_string());
            importable = false;
        }
        if route.model.is_none() {
            warnings.push("当前 Codex 自定义路由缺少主模型".to_string());
            importable = false;
        }
        if let Err(error) = crate::adapter::read_provider_parameters(app, text) {
            warnings.push(format!("供应商运行参数无法导入：{error}"));
            importable = false;
        }
        return importable;
    }
    let root: serde_json::Value =
        serde_json::from_str(text).expect("syntax checked before import inspection");
    let claude_import_error = import_models(&root, ModelSource::Client).err();
    let native = crate::claude_native::from_config(&root);
    if let Err(error) = &native { warnings.push(format!("Claude 原生云配置无法导入：{error}")); return false; }
    let mut importable = native.ok().flatten().is_some() || route.route_mode == RouteMode::Official
        || (route.base_url.is_some() && claude_import_error.is_none());
    if route.route_mode == RouteMode::Custom {
        if let Some(error) = claude_import_error {
            warnings.push(format!("当前 Claude 配置无法作为供应商档案导入：{error}"));
        }
    }
    if let Err(error) = crate::adapter::read_provider_parameters(app, text) {
        importable = false;
        warnings.push(format!("供应商运行参数无法导入：{error}"));
    }
    importable
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
    let managed = provider_id == crate::ownership::CODEX_PROVIDER_ID
        && doc
            .as_table()
            .get("openai_base_url")
            .and_then(Item::as_str)
            .is_some_and(|value| crate::adapter::codex::is_gateway_base_url(value));

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
