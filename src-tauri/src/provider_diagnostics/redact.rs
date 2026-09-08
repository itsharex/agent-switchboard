use serde_json::Value;

use asb_core::redact::REDACTED;

fn secret_name(name: &str) -> bool {
    let name: String = name
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .collect();
    matches!(
        name.to_ascii_lowercase().as_str(),
        "authorization"
            | "proxyauthorization"
            | "apikey"
            | "xapikey"
            | "accesstoken"
            | "refreshtoken"
            | "idtoken"
            | "token"
            | "password"
            | "secret"
            | "clientsecret"
            | "credential"
            | "credentials"
            | "cookie"
            | "setcookie"
    )
}

pub(super) fn endpoint(endpoint: &str) -> String {
    let Ok(mut url) = reqwest::Url::parse(endpoint) else {
        return redact_text(endpoint, &[]);
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    let pairs: Vec<_> = url
        .query_pairs()
        .map(|(key, value)| {
            let value = if secret_name(&key) {
                REDACTED.to_string()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect();
    if pairs.iter().any(|(key, _)| secret_name(key)) {
        url.query_pairs_mut().clear().extend_pairs(pairs);
    }
    url.to_string()
}

pub(super) fn body(text: &str, secrets: &[&str], truncated: bool) -> String {
    let Ok(mut value) = serde_json::from_str::<Value>(text) else {
        return redact_text(&redact_partial_tail(text, secrets, truncated), secrets);
    };
    redact_value(&mut value, secrets);
    serde_json::to_string(&value).unwrap_or_else(|_| redact_text(text, secrets))
}

fn redact_value(value: &mut Value, secrets: &[&str]) {
    match value {
        Value::Object(map) => {
            for (name, value) in map {
                if secret_name(name) {
                    *value = Value::String(REDACTED.to_string());
                } else {
                    redact_value(value, secrets);
                }
            }
        }
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| redact_value(value, secrets)),
        Value::String(value) => *value = redact_text(value, secrets),
        _ => {}
    }
}

pub(crate) fn redact_text(text: &str, secrets: &[&str]) -> String {
    let mut text = text.to_string();
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        text = text.replace(*secret, REDACTED);
        if let Ok(encoded) = serde_json::to_string(secret) {
            text = text.replace(&encoded[1..encoded.len() - 1], REDACTED);
        }
    }
    redact_assignments(&text)
}

fn redact_partial_tail(text: &str, secrets: &[&str], truncated: bool) -> String {
    if !truncated {
        return text.to_string();
    }
    let mut earliest = text.len();
    for secret in secrets.iter().filter(|secret| !secret.is_empty()) {
        let encoded = serde_json::to_string(secret).unwrap_or_default();
        let escaped = encoded
            .get(1..encoded.len().saturating_sub(1))
            .unwrap_or("");
        for variant in [*secret, escaped] {
            for (length, _) in variant.char_indices().skip(1) {
                if text.ends_with(&variant[..length]) {
                    earliest = earliest.min(text.len() - length);
                }
            }
        }
    }
    if earliest == text.len() {
        return text.to_string();
    }
    format!("{}{REDACTED}", &text[..earliest])
}

/// This also handles a truncated JSON body that cannot be parsed safely.
fn redact_assignments(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = String::new();
    let mut copied = 0;
    let mut index = 0;
    while index < bytes.len() {
        if !(bytes[index].is_ascii_alphabetic() || bytes[index] == b'_') {
            index += 1;
            continue;
        }
        let start = index;
        while index < bytes.len()
            && (bytes[index].is_ascii_alphanumeric() || b"_-".contains(&bytes[index]))
        {
            index += 1;
        }
        if !secret_name(&text[start..index]) {
            continue;
        }
        let mut value_start = index;
        if bytes
            .get(value_start)
            .is_some_and(|byte| *byte == b'"' || *byte == b'\'')
        {
            value_start += 1;
        }
        while bytes.get(value_start).is_some_and(u8::is_ascii_whitespace) {
            value_start += 1;
        }
        if !bytes
            .get(value_start)
            .is_some_and(|byte| *byte == b':' || *byte == b'=')
        {
            continue;
        }
        value_start += 1;
        while bytes.get(value_start).is_some_and(u8::is_ascii_whitespace) {
            value_start += 1;
        }
        let end = value_end(bytes, value_start);
        output.push_str(&text[copied..value_start]);
        output.push_str(REDACTED);
        copied = end;
        index = end;
    }
    output.push_str(&text[copied..]);
    output
}

fn value_end(bytes: &[u8], start: usize) -> usize {
    let quote = bytes
        .get(start)
        .copied()
        .filter(|byte| *byte == b'"' || *byte == b'\'');
    let mut end = start + usize::from(quote.is_some());
    while end < bytes.len() {
        if let Some(quote) = quote {
            if bytes[end] == b'\\' {
                end = (end + 2).min(bytes.len());
                continue;
            }
            if bytes[end] == quote {
                return end + 1;
            }
        } else if b"\r\n,;<>}".contains(&bytes[end]) {
            break;
        }
        end += 1;
    }
    end
}
