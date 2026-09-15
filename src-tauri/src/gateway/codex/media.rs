//! Reactive media downgrade for the Codex bridge: when an upstream rejects
//! image input, the failed attempt may be retried once with every structured
//! `input_image` item replaced by a text marker so the conversation survives
//! on a text-only model. Detection matches the observable failure shapes;
//! the sanitizer never invents model capabilities.

use crate::provider_diagnostics::ProviderDiagnostic;
use serde_json::{json, Value};

pub(crate) const UNSUPPORTED_IMAGE_MARKER: &str = "[Unsupported Image]";

/// Statuses an image-rejecting upstream actually returns; 401/429 and friends
/// are authentication or capacity problems, not modality rejections.
const REJECTION_STATUSES: [u16; 4] = [400, 415, 422, 501];

const IMAGE_WORDS: [&str; 8] = [
    "image",
    "vision",
    "multimodal",
    "multi-modal",
    "modality",
    "modalities",
    "media",
    "attachment",
];

/// Wording that asserts "text only" without naming images at all; several
/// relays phrase it that way.
const SELF_EVIDENT_HINTS: [&str; 2] = ["only support text", "only supports text"];

const UNSUPPORTED_HINTS: [&str; 15] = [
    "unsupported",
    "not supported",
    "does not support",
    "doesn't support",
    "do not support",
    "don't support",
    "text only",
    "text-only",
    "invalid content type",
    "invalid message content",
    "unknown variant",
    "unknown content type",
    "unrecognized content type",
    "cannot process",
    "can't process",
];

pub(crate) fn is_unsupported_image_failure(diagnostic: &ProviderDiagnostic) -> bool {
    let Some(status) = diagnostic.status else {
        return false;
    };
    if !REJECTION_STATUSES.contains(&status) {
        return false;
    }
    // The upstream's own error text lives in `body`; `message` is the
    // gateway's generic per-kind wording. Both are screened.
    let mut haystack = diagnostic.message.to_ascii_lowercase();
    if let Some(body) = &diagnostic.body {
        haystack.push('\n');
        haystack.push_str(&body.to_ascii_lowercase());
    }
    if SELF_EVIDENT_HINTS
        .iter()
        .any(|hint| haystack.contains(hint))
    {
        return true;
    }
    IMAGE_WORDS.iter().any(|word| haystack.contains(word))
        && UNSUPPORTED_HINTS.iter().any(|hint| haystack.contains(hint))
}

pub(crate) fn contains_images(value: &Value) -> bool {
    match value {
        Value::Object(object) => {
            object.get("type").and_then(Value::as_str) == Some("input_image")
                || object.values().any(contains_images)
        }
        Value::Array(items) => items.iter().any(contains_images),
        _ => false,
    }
}

/// Replaces every structured `input_image` content item with an `input_text`
/// marker and returns how many items changed. Media serialized inside JSON
/// strings (tool outputs) is left untouched: rewriting those would corrupt
/// payloads the client still owns.
pub(crate) fn replace_images_with_marker(value: &mut Value) -> usize {
    match value {
        Value::Object(object) => {
            if object.get("type").and_then(Value::as_str) == Some("input_image") {
                *value = json!({"type": "input_text", "text": UNSUPPORTED_IMAGE_MARKER});
                return 1;
            }
            object.values_mut().map(replace_images_with_marker).sum()
        }
        Value::Array(items) => items.iter_mut().map(replace_images_with_marker).sum(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(status: Option<u16>, message: &str) -> ProviderDiagnostic {
        let mut diagnostic = ProviderDiagnostic::new(
            crate::provider_diagnostics::ProviderFailureKind::Upstream,
            "https://relay.test",
            message,
        );
        diagnostic.status = status;
        diagnostic
    }

    #[test]
    fn rejection_detection_matches_status_and_wording_shapes() {
        for (status, message) in [
            (400, "This model does not support image input"),
            (422, "unknown variant `input_image`"),
            (415, "media type not supported"),
            (501, "vision capability is not supported here"),
            (400, "Model only supports text input"),
        ] {
            assert!(
                is_unsupported_image_failure(&diagnostic(Some(status), message)),
                "{status} {message}"
            );
        }
        for (status, message) in [
            (401, "image input is not supported"), // wrong status
            (429, "image capacity exhausted"),     // wrong status
            (400, "invalid api key"),              // no image word
            (400, "image quota exceeded"),         // image word, no hint
            (200, "image input is not supported"), // success shape
        ] {
            assert!(
                !is_unsupported_image_failure(&diagnostic(Some(status), message)),
                "{status} {message}"
            );
        }
        assert!(!is_unsupported_image_failure(&diagnostic(
            None,
            "image unsupported"
        )));
    }

    #[test]
    fn image_detection_and_marker_replacement_round_trip() {
        let body = json!({
            "model": "m",
            "input": [
                {"type": "message", "role": "user", "content": [
                    {"type": "input_text", "text": "look"},
                    {"type": "input_image", "image_url": "https://x.test/a.png"},
                ]},
                {"type": "message", "role": "user", "content": "plain"},
                {"type": "message", "role": "user", "content": [
                    {"type": "input_image", "image_url": "data:image/png;base64,AAAA"},
                ]},
            ]
        });
        assert!(contains_images(&body));
        let mut sanitized = body.clone();
        assert_eq!(replace_images_with_marker(&mut sanitized), 2);
        assert_eq!(
            sanitized["input"][0]["content"][1],
            json!({"type": "input_text", "text": UNSUPPORTED_IMAGE_MARKER})
        );
        assert_eq!(
            sanitized["input"][0]["content"][0],
            body["input"][0]["content"][0]
        );
        assert_eq!(sanitized["input"][1], body["input"][1]);
        assert!(!contains_images(&sanitized));
        assert_eq!(replace_images_with_marker(&mut sanitized), 0, "idempotent");
        let mut no_images =
            json!({"model":"m","input":[{"type":"message","role":"user","content":"hi"}]});
        assert!(!contains_images(&no_images));
        assert_eq!(replace_images_with_marker(&mut no_images), 0);
    }
}
