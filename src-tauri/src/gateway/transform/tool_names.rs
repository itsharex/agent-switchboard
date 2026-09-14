//! Reversible names for Responses namespace tools on protocols that expose a
//! flat function namespace.

use super::{error, ToolKind, TransformError, CODEX_TOOL_SEARCH_NAME};
use asb_core::contracts::UpstreamProtocol;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

const PREFIX: &str = "asbns_";

/// Renders a Responses `(namespace, name)` pair as a target function name.
///
/// A direct flat name stays readable where it is already portable. Everything
/// else uses a byte-length-delimited base64url envelope, so punctuation in a
/// namespace such as `functions.` remains reversible without relying on a
/// separator that could collide with either component.
pub(super) fn render_target_name(
    protocol: UpstreamProtocol,
    namespace: Option<&str>,
    name: &str,
    kind: ToolKind,
) -> Result<String, TransformError> {
    if name.is_empty() {
        return error("工具名不能为空");
    }
    if kind == ToolKind::ToolSearch {
        if namespace.is_some() || name != CODEX_TOOL_SEARCH_NAME {
            return error("tool_search 必须使用保留的平铺名称");
        }
        return Ok(CODEX_TOOL_SEARCH_NAME.to_string());
    }
    let limit = target_limit(protocol);
    let rendered = match namespace {
        None if is_direct_name(name, limit)
            && !name.starts_with(PREFIX)
            && name != CODEX_TOOL_SEARCH_NAME =>
        {
            name.to_string()
        }
        None => encode_flat_name(name),
        Some(namespace) if !namespace.is_empty() => encode_namespaced_name(namespace, name),
        Some(_) => return error("工具命名空间不能为空"),
    };
    let rendered = match kind {
        ToolKind::Function => rendered,
        // Custom calls must always retain a marker, including portable names,
        // because a later upstream response otherwise cannot distinguish them
        // from ordinary functions.
        ToolKind::Custom => format!("{PREFIX}c_{}", encode_flat_name(&rendered)),
        ToolKind::ToolSearch => unreachable!("handled before name encoding"),
    };
    if rendered.len() > limit {
        return error(format!(
            "工具名编码后为 {} 个字符，超过目标协议允许的 {} 个字符",
            rendered.len(),
            limit
        ));
    }
    Ok(rendered)
}

/// Decodes only values emitted by [`render_target_name`]. A source flat name
/// beginning with the reserved prefix is always escaped, so malformed or
/// forged envelopes are rejected instead of being treated as another tool.
pub(super) fn parse_target_name(
    name: &str,
) -> Result<(Option<String>, String, ToolKind), TransformError> {
    if name == CODEX_TOOL_SEARCH_NAME {
        return Ok((
            None,
            CODEX_TOOL_SEARCH_NAME.to_string(),
            ToolKind::ToolSearch,
        ));
    }
    let Some(encoded) = name.strip_prefix(PREFIX) else {
        return Ok((None, name.to_string(), ToolKind::Function));
    };
    if let Some(encoded) = encoded.strip_prefix("c_") {
        let (namespace, inner, kind) = parse_target_name(encoded)?;
        if kind != ToolKind::Function {
            return error("custom 工具名编码无效");
        }
        return Ok((namespace, inner, ToolKind::Custom));
    }
    if let Some(encoded) = encoded.strip_prefix("f_") {
        return Ok((
            None,
            decode_component(encoded, "工具名")?,
            ToolKind::Function,
        ));
    }
    let Some(encoded) = encoded.strip_prefix("n_") else {
        return error("命名空间工具名编码无效");
    };
    let (namespace, encoded) = decode_prefixed_component(encoded, "工具命名空间")?;
    let name = decode_component(encoded, "工具名")?;
    Ok((Some(namespace), name, ToolKind::Function))
}

fn encode_flat_name(name: &str) -> String {
    let encoded = URL_SAFE_NO_PAD.encode(name.as_bytes());
    format!("{PREFIX}f_{}_{encoded}", name.len())
}

fn encode_namespaced_name(namespace: &str, name: &str) -> String {
    let namespace_encoded = URL_SAFE_NO_PAD.encode(namespace.as_bytes());
    let name_encoded = URL_SAFE_NO_PAD.encode(name.as_bytes());
    format!(
        "{PREFIX}n_{}_{}_{}_{}",
        namespace.len(),
        namespace_encoded,
        name.len(),
        name_encoded,
    )
}

fn decode_prefixed_component<'a>(
    value: &'a str,
    label: &str,
) -> Result<(String, &'a str), TransformError> {
    let (length, remainder) = split_length(value)?;
    let encoded_length = base64_length(length)?;
    if remainder.len() <= encoded_length || remainder.as_bytes()[encoded_length] != b'_' {
        return error("命名空间工具名编码无效");
    }
    let component = decode_exact_component(&remainder[..encoded_length], length, label)?;
    Ok((component, &remainder[encoded_length + 1..]))
}

fn decode_component(value: &str, label: &str) -> Result<String, TransformError> {
    let (length, encoded) = split_length(value)?;
    let encoded_length = base64_length(length)?;
    if encoded.len() != encoded_length {
        return error("命名空间工具名编码无效");
    }
    decode_exact_component(encoded, length, label)
}

fn split_length(value: &str) -> Result<(usize, &str), TransformError> {
    let (length, remainder) = value
        .split_once('_')
        .ok_or_else(|| TransformError("命名空间工具名编码无效".to_string()))?;
    if length.is_empty() || !length.bytes().all(|byte| byte.is_ascii_digit()) {
        return error("命名空间工具名编码无效");
    }
    let length = length
        .parse::<usize>()
        .map_err(|_| TransformError("命名空间工具名编码无效".to_string()))?;
    if length == 0 {
        return error("命名空间工具名编码无效");
    }
    Ok((length, remainder))
}

fn decode_exact_component(
    encoded: &str,
    expected_length: usize,
    label: &str,
) -> Result<String, TransformError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| TransformError("命名空间工具名编码无效".to_string()))?;
    if bytes.len() != expected_length {
        return error("命名空间工具名编码无效");
    }
    String::from_utf8(bytes).map_err(|_| TransformError(format!("{label} 必须是 UTF-8 文本")))
}

fn base64_length(length: usize) -> Result<usize, TransformError> {
    length
        .checked_div(3)
        .and_then(|quotient| {
            quotient.checked_mul(4).and_then(|whole| {
                whole.checked_add(match length % 3 {
                    0 => 0,
                    1 => 2,
                    _ => 3,
                })
            })
        })
        .ok_or_else(|| TransformError("命名空间工具名编码无效".to_string()))
}

fn target_limit(protocol: UpstreamProtocol) -> usize {
    match protocol {
        // Chat Completions has the smallest portable function-name bound.
        UpstreamProtocol::ChatCompletions | UpstreamProtocol::GeminiGenerateContent => 64,
        UpstreamProtocol::Responses | UpstreamProtocol::AnthropicMessages => 128,
    }
}

fn is_direct_name(value: &str, limit: usize) -> bool {
    value.len() <= limit
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_a_portable_flat_name() {
        assert_eq!(
            render_target_name(
                UpstreamProtocol::ChatCompletions,
                None,
                "weather",
                ToolKind::Function
            )
            .unwrap(),
            "weather"
        );
    }

    #[test]
    fn round_trips_a_punctuated_namespace_and_name() {
        let rendered = render_target_name(
            UpstreamProtocol::ChatCompletions,
            Some("functions."),
            "apply.patch",
            ToolKind::Function,
        )
        .unwrap();
        assert_eq!(
            parse_target_name(&rendered).unwrap(),
            (
                Some("functions.".to_string()),
                "apply.patch".to_string(),
                ToolKind::Function
            )
        );
    }

    #[test]
    fn escapes_a_flat_name_that_uses_the_reserved_prefix() {
        let rendered = render_target_name(
            UpstreamProtocol::AnthropicMessages,
            None,
            "asbns_regular_tool",
            ToolKind::Function,
        )
        .unwrap();
        assert_ne!(rendered, "asbns_regular_tool");
        assert_eq!(
            parse_target_name(&rendered).unwrap().1,
            "asbns_regular_tool"
        );
    }

    #[test]
    fn rejects_a_malformed_envelope() {
        assert!(parse_target_name("asbns_n_4_dGVzdA_4_Zm9v").is_err());
    }

    #[test]
    fn custom_tool_names_are_always_tagged_and_reversible() {
        let rendered = render_target_name(
            UpstreamProtocol::ChatCompletions,
            None,
            "apply_patch",
            ToolKind::Custom,
        )
        .unwrap();
        assert!(rendered.starts_with("asbns_c_"));
        assert_eq!(
            parse_target_name(&rendered).unwrap(),
            (None, "apply_patch".to_string(), ToolKind::Custom)
        );
    }
}
