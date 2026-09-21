use std::collections::BTreeSet;

use serde_json::Value;

use crate::contracts::UsageQuery;

/// Usage-script keys which represent an independent endpoint or credential.
/// The profile's own `baseUrl` and `apiKey` are the only supported query
/// inputs, so importing one of these would silently change the query's owner.
const USAGE_SCRIPT_INPUT_OVERRIDES: [&str; 7] = [
    "apiKey",
    "baseUrl",
    "accessToken",
    "userId",
    "accessKeyId",
    "secretAccessKey",
    "teamId",
];

const USAGE_SCRIPT_REPRESENTED_KEYS: [&str; 5] =
    ["enabled", "language", "code", "templateType", "timeout"];

/// Native-script syntheses for the source's built-in usage templates. Those
/// rows carry empty `code`; each vendored program uses only the profile's own
/// `baseUrl` and `apiKey` inputs and documents its reference baseline in the
/// file itself.
const GENERAL_TEMPLATE_SCRIPT: &str = include_str!("templates/general_usage.js");
const BALANCE_TEMPLATE_SCRIPT: &str = include_str!("templates/balance_usage.js");
const ZHIPU_TOKEN_PLAN_SCRIPT: &str = include_str!("templates/zhipu_token_plan_usage.js");
const ZHIPU_TEAM_TOKEN_PLAN_SCRIPT: &str = include_str!("templates/zhipu_team_token_plan_usage.js");
const KIMI_TOKEN_PLAN_SCRIPT: &str = include_str!("templates/kimi_token_plan_usage.js");
const MINIMAX_TOKEN_PLAN_SCRIPT: &str = include_str!("templates/minimax_token_plan_usage.js");
const ZENMUX_TOKEN_PLAN_SCRIPT: &str = include_str!("templates/zenmux_token_plan_usage.js");
const OPENCODE_GO_TOKEN_PLAN_SCRIPT: &str =
    include_str!("templates/opencode_go_token_plan_usage.js");

/// One non-empty trimmed identifier from the source usage-script object.
fn optional_usage_script_string(
    script: &serde_json::Map<String, Value>,
    key: &str,
) -> Option<String> {
    script
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Embeds one source identifier inside a JavaScript string literal of a
/// synthesized program.
fn javascript_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            other => escaped.push(other),
        }
    }
    escaped
}

/// Converts one enabled source JavaScript program into the application's
/// native script contract. This is an import-time compilation step: durable
/// profiles never retain a source-specific query kind or a runtime compatibility
/// branch.
fn compile_usage_script(source: &str, token_plan: bool) -> String {
    include_str!("templates/script_adapter.js")
        .replace("__ASB_TOKEN_PLAN__", if token_plan { "true" } else { "false" })
        .replace("__ASB_CC_SOURCE__", source)
}

fn contains_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(text) => !text.trim().is_empty(),
        _ => true,
    }
}

fn has_unsupported_placeholder(source: &str) -> bool {
    let mut remaining = source;
    while let Some(start) = remaining.find("{{") {
        let after_start = &remaining[start + 2..];
        let Some(end) = after_start.find("}}") else {
            break;
        };
        let name = after_start[..end].trim();
        if name != "apiKey" && name != "baseUrl" {
            return true;
        }
        remaining = &after_start[end + 2..];
    }
    false
}

/// Resolves the only query-script source supported by this import.
/// Warnings name source fields only; values and script text never appear in
/// diagnostics or scan data.
pub(super) fn map_usage_query(
    meta: Option<&str>,
    warnings: &mut Vec<String>,
) -> Option<UsageQuery> {
    let Some(text) = meta.filter(|text| !text.trim().is_empty()) else {
        return None;
    };
    let meta: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(_) => {
            warnings.push("未导入: meta.usage_script（元数据无法解析）".to_string());
            return None;
        }
    };
    let Some(script) = meta.get("usage_script") else {
        return None;
    };
    let Some(script) = script.as_object() else {
        warnings.push("未导入: meta.usage_script（格式无效）".to_string());
        return None;
    };
    if script.get("enabled").and_then(Value::as_bool) != Some(true) {
        warnings.push("未导入: meta.usage_script（脚本已禁用）".to_string());
        return None;
    }
    if !script
        .get("language")
        .and_then(Value::as_str)
        .is_some_and(|language| language.eq_ignore_ascii_case("javascript"))
    {
        warnings.push("未导入: meta.usage_script（仅支持 JavaScript 脚本）".to_string());
        return None;
    }
    for key in USAGE_SCRIPT_INPUT_OVERRIDES {
        if script.get(key).is_some_and(contains_value) {
            warnings.push(format!("未导入: meta.usage_script.{key}"));
            return None;
        }
    }
    let code = script
        .get("code")
        .and_then(Value::as_str)
        .filter(|source| !source.trim().is_empty());
    let mut consumed_keys: Vec<&str> = Vec::new();
    let query = match code {
        Some(source) => {
            if has_unsupported_placeholder(source) {
                warnings
                    .push("未导入: meta.usage_script.code（包含无对应输入的占位符）".to_string());
                return None;
            }
            Some(compile_usage_script(source,
                script.get("templateType").and_then(Value::as_str) == Some("token_plan")))
        }
        None => match script.get("templateType").and_then(Value::as_str) {
            // Built-in templates carry no code: the pinned reference
            // baseline's endpoints and parsing are vendored below as native
            // request/extract programs.
            Some("general") => Some(GENERAL_TEMPLATE_SCRIPT.to_string()),
            Some("balance") => Some(BALANCE_TEMPLATE_SCRIPT.to_string()),
            Some("token_plan") => match script.get("codingPlanProvider").and_then(Value::as_str) {
                Some("zhipu") => {
                    consumed_keys.push("codingPlanProvider");
                    Some(ZHIPU_TOKEN_PLAN_SCRIPT.to_string())
                }
                Some("zhipu_team") => match (
                    optional_usage_script_string(script, "teamOrganizationId"),
                    optional_usage_script_string(script, "teamProjectId"),
                ) {
                    (Some(organization), Some(project)) => {
                        consumed_keys.push("codingPlanProvider");
                        consumed_keys.push("teamOrganizationId");
                        consumed_keys.push("teamProjectId");
                        Some(
                            ZHIPU_TEAM_TOKEN_PLAN_SCRIPT
                                .replace(
                                    "__ASB_ZHIPU_TEAM_ORG__",
                                    &javascript_string(&organization),
                                )
                                .replace(
                                    "__ASB_ZHIPU_TEAM_PROJECT__",
                                    &javascript_string(&project),
                                ),
                        )
                    }
                    _ => {
                        warnings.push(
                            "未导入: meta.usage_script（zhipu_team 模板需要 teamOrganizationId 与 teamProjectId）"
                                .to_string(),
                        );
                        None
                    }
                },
                Some("kimi") => {
                    consumed_keys.push("codingPlanProvider");
                    Some(KIMI_TOKEN_PLAN_SCRIPT.to_string())
                }
                Some("minimax") => {
                    consumed_keys.push("codingPlanProvider");
                    Some(MINIMAX_TOKEN_PLAN_SCRIPT.to_string())
                }
                Some("zenmux") => {
                    consumed_keys.push("codingPlanProvider");
                    Some(ZENMUX_TOKEN_PLAN_SCRIPT.to_string())
                }
                Some("opencode_go") => {
                    consumed_keys.push("codingPlanProvider");
                    Some(OPENCODE_GO_TOKEN_PLAN_SCRIPT.to_string())
                }
                Some("volcengine") => {
                    warnings.push(
                        "未导入: meta.usage_script（volcengine 的火山方舟额度依赖账号 AccessKey ID/Secret 而非推理 API 密钥，本应用只支持档案自有凭据的查询）"
                            .to_string(),
                    );
                    None
                }
                Some(provider) => {
                    warnings.push(format!(
                        "未导入: meta.usage_script.templateType（未知 Coding Plan 供应商 {provider}）"
                    ));
                    None
                }
                None => {
                    warnings.push(
                        "未导入: meta.usage_script（token_plan 模板缺少 codingPlanProvider）"
                            .to_string(),
                    );
                    None
                }
            },
            Some("newapi") => {
                warnings.push(
                    "未导入: meta.usage_script（newapi 模板依赖站点独立凭据，本应用只支持档案自有凭据的查询）"
                        .to_string(),
                );
                None
            }
            Some("github_copilot") => {
                warnings.push(
                    "未导入: meta.usage_script（github_copilot 模板依赖 GitHub 登录凭据，本应用只支持档案自有凭据的查询）"
                        .to_string(),
                );
                None
            }
            Some("official_subscription") => {
                warnings.push(
                    "未导入: meta.usage_script（官方订阅额度由官方登录档案的原生额度面板支持）"
                        .to_string(),
                );
                None
            }
            Some("custom") | None => {
                warnings.push("未导入: meta.usage_script.code（没有可导入的源码）".to_string());
                None
            }
            Some(other) => {
                warnings.push(format!(
                    "未导入: meta.usage_script.templateType（未知模板 {other}）"
                ));
                None
            }
        },
    }?;

    let mut unsupported = BTreeSet::new();
    for key in script.keys() {
        if !USAGE_SCRIPT_REPRESENTED_KEYS.contains(&key.as_str())
            && !USAGE_SCRIPT_INPUT_OVERRIDES.contains(&key.as_str())
            && !consumed_keys.contains(&key.as_str())
        {
            unsupported.insert(key);
        }
    }
    for key in unsupported {
        warnings.push(format!("未导入: meta.usage_script.{key}"));
    }
    if script.get("timeout").is_some_and(contains_value) {
        warnings.push("未导入: meta.usage_script.timeout".to_string());
    }
    Some(UsageQuery::Script {
        source: query,
        // The source interval lands in the unsupported-key warnings; imported
        // queries start manual-only.
        refresh_interval_minutes: 0,
    })
}
