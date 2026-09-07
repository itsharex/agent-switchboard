use serde_json::Value as JsonValue;

use crate::extensions::mcp::EntryChange;
use crate::redact;

/// Renders a change's before/after values for display, redacting
/// secret-shaped content: values under secret-shaped keys, live-secret
/// shapes anywhere in the text, and whole entries that are themselves
/// secret material. Used by every preview builder so no view can forget
/// the masking step.
pub fn redact_change(change: &EntryChange) -> (Option<String>, Option<String>) {
    let key = change.pointer.rsplit('.').next().unwrap_or(&change.pointer);
    let redact_text = |text: &Option<String>| -> Option<String> {
        text.as_ref().map(|value| match redact::redact(key, value) {
            masked if masked == redact::REDACTED => masked,
            _ => redact_rendered_entry(value),
        })
    };
    (redact_text(&change.before), redact_text(&change.after))
}

/// Redacts values inside a rendered server entry so previews of full
/// entries (Codex tables, JSON objects) never leak resolved credentials,
/// private endpoints, or argument values. The persisted render remains
/// untouched; this function is exclusively for display projections.
pub fn redact_rendered_entry(text: &str) -> String {
    if let Ok(mut value) = serde_json::from_str::<JsonValue>(text) {
        redact_json_value(&mut value, None, false);
        return serde_json::to_string(&value).unwrap_or_else(|_| redact::REDACTED.to_string());
    }

    redact_toml_entry(text)
}

fn redact_json_value(value: &mut JsonValue, key: Option<&str>, inherited_sensitive: bool) {
    let key_sensitive = key.is_some_and(is_sensitive_container);
    let sensitive = inherited_sensitive || key_sensitive;
    match value {
        JsonValue::Object(values) => {
            for (name, child) in values {
                redact_json_value(child, Some(name), sensitive);
            }
        }
        JsonValue::Array(values) => {
            for child in values {
                redact_json_value(child, key, sensitive);
            }
        }
        JsonValue::String(text) => {
            if sensitive
                || key.is_some_and(is_display_sensitive_key)
                || is_endpoint(text)
                || redact::is_secret_value(text)
            {
                *text = redact::REDACTED.to_string();
            }
        }
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => {}
    }
}

fn redact_toml_entry(text: &str) -> String {
    let mut lines = Vec::new();
    let mut in_sensitive_table = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_sensitive_table = is_sensitive_container(
                trimmed.trim_start_matches('[').trim_end_matches(']').trim(),
            );
            lines.push(line.to_string());
            continue;
        }
        let masked = match line.split_once('=') {
            Some((prefix, raw_value)) => {
                let key = prefix.trim().trim_matches('"');
                let candidate = raw_value.trim().trim_matches('"').trim_matches(',');
                if in_sensitive_table
                    || is_display_sensitive_key(key)
                    || is_endpoint(candidate)
                    || redact::is_secret_value(candidate)
                {
                    format!("{prefix}= {}", redact::REDACTED)
                } else {
                    line.to_string()
                }
            }
            None => line.to_string(),
        };
        lines.push(masked);
    }
    lines.join("\n")
}

fn is_sensitive_container(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key == "env"
        || key.ends_with(".env")
        || key.contains("header")
        || key.contains("bearer")
        || key == "args"
}

fn is_display_sensitive_key(key: &str) -> bool {
    is_sensitive_container(key)
        || redact::is_secret_key(key)
        || matches!(
            key.to_ascii_lowercase().as_str(),
            "url" | "endpoint" | "base_url" | "baseurl"
        )
}

fn is_endpoint(value: &str) -> bool {
    let lower = value.trim().trim_matches('"').to_ascii_lowercase();
    lower.starts_with("https://")
        || lower.starts_with("http://")
        || lower.starts_with("wss://")
        || lower.starts_with("ws://")
}
