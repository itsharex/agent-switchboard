//! Secret redaction.
//!
//! Every value that leaves the core toward previews, diffs, errors, or logs
//! passes through this module. Secret-shaped keys render as one stable
//! token, never a partially visible value.

/// The one redaction marker used everywhere.
pub const REDACTED: &str = "••••••••";

/// Key fragments that mark a value as secret-shaped. Matched
/// case-insensitively against the dotted key path.
const SECRET_MARKERS: &[&str] = &[
    "token",
    "secret",
    "api_key",
    "access_key",
    "apikey",
    "credential",
    "auth",
];

/// True when a key path names something secret-like. The first app-specific
/// spec wins; keys are disjoint across apps today (only `model` repeats, as
/// String on both), so no classification depends on the loop order.
pub fn is_secret_key(key: &str) -> bool {
    for app in [crate::AppKind::Codex, crate::AppKind::Claude] {
        if let Some(spec) = crate::ownership::setting_spec(app, key) {
            return spec.value_type == crate::ownership::SettingValueType::Secret;
        }
    }
    let lower = key.to_ascii_lowercase();
    SECRET_MARKERS.iter().any(|m| lower.contains(m))
}

/// Value prefixes that mark a value as a live secret, whatever the key says.
/// The trailing space is load-bearing: linkers may lay this literal adjacent
/// to other string data in the binary, and a bare final `AIza` glued to
/// following word bytes would false-positive release credential scans.
const SECRET_VALUE_PREFIXES: &str = "sk- ghp_ gho_ github_pat_ xox AKIA AIza ";

/// True for credential-bearing URLs, known token prefixes, or long
/// pure-alphanumeric runs that no model name or ordinary URL would produce.
pub fn is_secret_value(value: &str) -> bool {
    if url::Url::parse(value).ok().is_some_and(|url| !url.username().is_empty() || url.password().is_some()) {
        return true;
    }
    if SECRET_VALUE_PREFIXES
        .split_ascii_whitespace()
        .any(|prefix| value.starts_with(prefix))
    {
        return true;
    }
    value.len() >= 32 && value.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Renders `value` for display: the stable redaction token when the key is
/// secret-shaped or the value itself looks like a leaked token, the raw text
/// otherwise.
pub fn redact(key: &str, value: &str) -> String {
    if is_secret_key(key) || is_secret_value(value) {
        REDACTED.to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credential_urls_are_secret_regardless_of_the_field_name() {
        for value in ["http://user:password@proxy.example:8080", "https://user@proxy.example", "socks5://:password@proxy.example"] {
            assert_eq!(redact("proxy", value), REDACTED);
        }
        assert_eq!(redact("proxy", "http://proxy.example:8080"), "http://proxy.example:8080");
    }

    #[test]
    fn secret_keys_render_a_stable_token() {
        assert_eq!(redact("env.ANTHROPIC_AUTH_TOKEN", "sk-live-abc"), REDACTED);
        assert_eq!(redact("experimental_bearer_token", "zzz"), REDACTED);
        // The token is stable across calls so diffs do not flicker.
        assert_eq!(redact("api_key", "a"), redact("api_key", "b"));
    }

    #[test]
    fn ordinary_values_render_verbatim() {
        assert_eq!(redact("hide_agent_reasoning", "true"), "true");
        assert_eq!(redact("model", "gpt-5"), "gpt-5");
        assert_eq!(
            redact("model", "claude-sonnet-4-20250514"),
            "claude-sonnet-4-20250514"
        );
        assert_eq!(redact("env.ANTHROPIC_BASE_URL", "https://x/"), "https://x/");
    }

    #[test]
    fn token_shaped_values_redact_even_under_ordinary_keys() {
        assert_eq!(
            redact("experimental_bearer_token", "sk-live-0123456789abcdef"),
            REDACTED
        );
        let long_key = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";
        assert_eq!(redact("some_key", long_key), REDACTED);
    }

    #[test]
    fn every_known_value_prefix_is_redacted() {
        for prefix in SECRET_VALUE_PREFIXES.split_ascii_whitespace() {
            assert_eq!(
                redact("provider_value", &format!("{prefix}value")),
                REDACTED
            );
        }
    }
    #[test]
    fn codex_routing_urls_render_verbatim() {
        // openai_base_url is a routing fact, not a secret: the route card
        // parses its host, so whole-value redaction there broke the display.
        assert_eq!(
            redact("openai_base_url", "https://relay.example/v1"),
            "https://relay.example/v1"
        );
        // A secret-shaped value still redacts under this key.
        assert_eq!(redact("openai_base_url", "sk-live-0123456789abcdef"), REDACTED);
    }
}
