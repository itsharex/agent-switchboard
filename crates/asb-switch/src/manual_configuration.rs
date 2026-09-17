//! Rehydrates a redacted client-configuration draft without exposing secrets.
//!
//! The renderer edits only the display copy. A preserved redaction marker at
//! the same structural location is restored from the live source before any
//! candidate is validated or written. New values remain user-provided values.

use asb_core::{redact, AppKind};
use serde_json::Value as Json;
use toml_edit::{DocumentMut, Item, Value as TomlValue};

use crate::SwitchError;

fn rejected(message: impl Into<String>) -> SwitchError {
    SwitchError::PlanRejected {
        message: message.into(),
        line: None,
    }
}

fn join_path(prefix: &str, key: &str) -> String {
    if prefix.is_empty() {
        key.to_string()
    } else {
        format!("{prefix}.{key}")
    }
}

/// Restores live secret values into a display-copy edit. A display marker may
/// only remain at the same source path; a marker introduced elsewhere is
/// rejected rather than being persisted as a real configuration value.
pub fn rehydrate_display_content(
    app: AppKind,
    current: &str,
    edited_display: &str,
) -> Result<String, SwitchError> {
    match app {
        AppKind::Claude => rehydrate_json(current, edited_display),
        AppKind::Codex => rehydrate_toml(current, edited_display),
    }
}

fn rehydrate_json(current: &str, edited_display: &str) -> Result<String, SwitchError> {
    let source = if current.is_empty() { "{}" } else { current };
    let source = serde_json::from_str::<Json>(source)
        .map_err(|_| rejected("真实 JSON 配置无法安全恢复，请先使用修复流程"))?;
    let mut edited = serde_json::from_str::<Json>(edited_display)
        .map_err(|_| rejected("手动配置草稿不是有效 JSON"))?;
    if !source.is_object() || !edited.is_object() {
        return Err(rejected("手动配置草稿的 JSON 根节点必须是对象"));
    }
    restore_json(&source, &mut edited, "", false);
    if contains_json_marker(&edited) {
        return Err(rejected("脱敏标记只能保留在原有敏感字段中，不能移动或新建"));
    }
    serde_json::to_string_pretty(&edited)
        .map_err(|_| rejected("无法生成手动 JSON 配置候选"))
}

fn restore_json(source: &Json, edited: &mut Json, path: &str, force_secret: bool) {
    let source_is_secret = force_secret || source.as_str().is_some_and(redact::is_secret_value);
    if source_is_secret {
        if edited.as_str() == Some(redact::REDACTED) {
            *edited = source.clone();
        }
        return;
    }
    match (source, edited) {
        (Json::Object(source_entries), Json::Object(edited_entries)) => {
            for (key, source_value) in source_entries {
                let Some(edited_value) = edited_entries.get_mut(key) else {
                    continue;
                };
                let value_path = join_path(path, key);
                restore_json(
                    source_value,
                    edited_value,
                    &value_path,
                    redact::is_secret_key(&value_path),
                );
            }
        }
        (Json::Array(source_entries), Json::Array(edited_entries)) => {
            for (source_value, edited_value) in source_entries.iter().zip(edited_entries.iter_mut()) {
                restore_json(source_value, edited_value, path, force_secret);
            }
        }
        _ => {}
    }
}

fn contains_json_marker(value: &Json) -> bool {
    match value {
        Json::String(value) => value == redact::REDACTED,
        Json::Array(values) => values.iter().any(contains_json_marker),
        Json::Object(entries) => entries.values().any(contains_json_marker),
        _ => false,
    }
}

fn rehydrate_toml(current: &str, edited_display: &str) -> Result<String, SwitchError> {
    let source = current
        .parse::<DocumentMut>()
        .map_err(|_| rejected("真实 TOML 配置无法安全恢复，请先使用修复流程"))?;
    let mut edited = edited_display
        .parse::<DocumentMut>()
        .map_err(|_| rejected("手动配置草稿不是有效 TOML"))?;
    restore_toml_item(source.as_item(), edited.as_item_mut(), "", false);
    if contains_toml_marker(edited.as_item()) {
        return Err(rejected("脱敏标记只能保留在原有敏感字段中，不能移动或新建"));
    }
    Ok(edited.to_string())
}

fn restore_toml_item(source: &Item, edited: &mut Item, path: &str, force_secret: bool) {
    if let (Some(source), Some(edited)) = (source.as_value(), edited.as_value_mut()) {
        restore_toml_value(source, edited, path, force_secret);
        return;
    }
    let Some(source_table) = source.as_table_like() else {
        return;
    };
    let Some(edited_table) = edited.as_table_like_mut() else {
        return;
    };
    for (key, source_item) in source_table.iter() {
        let Some(edited_item) = edited_table.get_mut(key) else {
            continue;
        };
        let item_path = join_path(path, key);
        restore_toml_item(
            source_item,
            edited_item,
            &item_path,
            force_secret || redact::is_secret_key(&item_path),
        );
    }
}

fn restore_toml_value(source: &TomlValue, edited: &mut TomlValue, path: &str, force_secret: bool) {
    let source_is_secret = force_secret || source.as_str().is_some_and(redact::is_secret_value);
    if source_is_secret {
        if edited.as_str() == Some(redact::REDACTED) {
            *edited = source.clone();
        }
        return;
    }
    match (source, edited) {
        (TomlValue::Array(source_entries), TomlValue::Array(edited_entries)) => {
            for (source_value, edited_value) in source_entries.iter().zip(edited_entries.iter_mut()) {
                restore_toml_value(source_value, edited_value, path, force_secret);
            }
        }
        (TomlValue::InlineTable(source_entries), TomlValue::InlineTable(edited_entries)) => {
            for (key, source_value) in source_entries.iter() {
                let Some(edited_value) = edited_entries.get_mut(key) else {
                    continue;
                };
                let value_path = join_path(path, key);
                restore_toml_value(
                    source_value,
                    edited_value,
                    &value_path,
                    force_secret || redact::is_secret_key(&value_path),
                );
            }
        }
        _ => {}
    }
}

fn contains_toml_marker(item: &Item) -> bool {
    if let Some(value) = item.as_value() {
        return contains_toml_marker_value(value);
    }
    item.as_table_like()
        .is_some_and(|table| table.iter().any(|(_, item)| contains_toml_marker(item)))
}

fn contains_toml_marker_value(value: &TomlValue) -> bool {
    if value.as_str() == Some(redact::REDACTED) {
        return true;
    }
    if let Some(values) = value.as_array() {
        return values.iter().any(contains_toml_marker_value);
    }
    value.as_inline_table().is_some_and(|table| {
        table.iter().any(|(_, value)| contains_toml_marker_value(value))
    })
}


/// Produces a semantics-preserving repair candidate for a malformed client
/// file. Repairs are intentionally narrow: UTF-8 BOM removal for both files,
/// plus JSON comments and trailing commas for Claude. Ambiguous syntax is
/// rejected rather than reconstructed and risking unrelated user content.
pub fn repair_invalid_configuration(app: AppKind, current: &str) -> Result<String, SwitchError> {
    let without_bom = current.strip_prefix('\u{feff}').unwrap_or(current);
    let candidate = match app {
        AppKind::Codex => without_bom.to_string(),
        AppKind::Claude => normalize_json(without_bom),
    };
    if candidate == current {
        return Err(rejected("当前错误无法安全自动修复；请先在客户端外部修正文件后重试"));
    }
    asb_core::adapter::validate_syntax(app, &candidate).map_err(|error| SwitchError::PlanRejected {
        message: "当前错误无法安全自动修复；请先在客户端外部修正文件后重试".to_string(),
        line: error.line,
    })?;
    Ok(candidate)
}

fn normalize_json(input: &str) -> String {
    let without_comments = strip_json_comments(input);
    strip_json_trailing_commas(&without_comments)
}

fn strip_json_comments(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < chars.len() {
        let current = chars[index];
        if in_string {
            output.push(current);
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if current == '"' {
            in_string = true;
            output.push(current);
            index += 1;
            continue;
        }
        if current == '/' && chars.get(index + 1) == Some(&'/') {
            index += 2;
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
            }
            continue;
        }
        if current == '/' && chars.get(index + 1) == Some(&'*') {
            index += 2;
            while index + 1 < chars.len() && !(chars[index] == '*' && chars[index + 1] == '/') {
                if chars[index] == '\n' {
                    output.push('\n');
                }
                index += 1;
            }
            index = (index + 2).min(chars.len());
            continue;
        }
        output.push(current);
        index += 1;
    }
    output
}

fn strip_json_trailing_commas(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    let mut in_string = false;
    let mut escaped = false;
    while index < chars.len() {
        let current = chars[index];
        if in_string {
            output.push(current);
            if escaped {
                escaped = false;
            } else if current == '\\' {
                escaped = true;
            } else if current == '"' {
                in_string = false;
            }
            index += 1;
            continue;
        }
        if current == '"' {
            in_string = true;
            output.push(current);
            index += 1;
            continue;
        }
        if current == ',' {
            let mut next = index + 1;
            while next < chars.len() && chars[next].is_whitespace() {
                next += 1;
            }
            if matches!(chars.get(next), Some(&'}') | Some(&']')) {
                index += 1;
                continue;
            }
        }
        output.push(current);
        index += 1;
    }
    output
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_manual_edit_restores_retained_secret_markers() {
        let source = r#"{"env":{"AUTH_TOKEN":"sk-live-secret"},"other":1}"#;
        let display = crate::display_content(AppKind::Claude, source);
        let edited = display.replace("\"other\": 1", "\"other\": 2");
        let rendered = rehydrate_display_content(AppKind::Claude, source, &edited).expect("rehydrate");
        assert!(rendered.contains("sk-live-secret"));
        assert!(rendered.contains("\"other\": 2"));
    }

    #[test]
    fn toml_manual_edit_restores_retained_secret_markers() {
        let source = "token = \"sk-live-secret\"\nother = 1\n";
        let display = crate::display_content(AppKind::Codex, source);
        let edited = display.replace("other = 1", "other = 2");
        let rendered = rehydrate_display_content(AppKind::Codex, source, &edited).expect("rehydrate");
        assert!(rendered.contains("sk-live-secret"));
        assert!(rendered.contains("other = 2"));
    }

    #[test]
    fn a_new_redaction_marker_is_rejected() {
        let error = rehydrate_display_content(AppKind::Claude, r#"{"other":"value"}"#, r#"{"other":"••••••••"}"#)
            .expect_err("new marker must be rejected");
        assert!(error.to_string().contains("脱敏标记"));
    }

    #[test]
    fn json_comments_and_trailing_commas_have_a_safe_repair() {
        let rendered = repair_invalid_configuration(AppKind::Claude, "{\n// note\n\"value\": 1,\n}\n")
            .expect("repair");
        assert_eq!(rendered, "{\n\n\"value\": 1\n}\n");
        assert!(asb_core::adapter::validate_syntax(AppKind::Claude, &rendered).is_ok());
    }
}
