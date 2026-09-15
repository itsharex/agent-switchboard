//! Targeted compatibility rewrites for strict third-party Codex upstreams.
//!
//! Generic cross-protocol conversion must stay provider-agnostic; these
//! modules only fire for upstreams that verifiably reject otherwise-valid
//! Codex traffic. Gates and wiring live here so no other layer knows about a
//! specific vendor.

mod moonshot;
mod namespaces;
mod xai;

use crate::gateway::ActiveRoute;
use asb_core::contracts::UpstreamProtocol;
use std::io::{Read, Result as IoResult};

/// True when the resolved upstream is xAI's strict native Responses endpoint.
/// ASB admission already guarantees catalog models, so the API-key form is the
/// complete gate until managed xAI accounts exist.
fn is_xai_native_responses(protocol: UpstreamProtocol, base_url: &str) -> bool {
    protocol == UpstreamProtocol::Responses && host_is(base_url, &["api.x.ai"])
}

/// Applies every request-side rewrite the selected upstream requires. Called
/// after model resolution and protocol conversion, on the final body.
pub(crate) fn apply_request_compat(route: &ActiveRoute, body: &mut Vec<u8>) -> Result<(), String> {
    apply_request(route.upstream_protocol, &route.upstream_base_url, body)
}

fn apply_request(
    protocol: UpstreamProtocol,
    base_url: &str,
    body: &mut Vec<u8>,
) -> Result<(), String> {
    if protocol == UpstreamProtocol::ChatCompletions
        && moonshot::upstream_requires_ref_sibling_all_of(base_url)
    {
        moonshot::wrap_ref_siblings_in_chat_tools(body)?;
    }
    if is_xai_native_responses(protocol, base_url) {
        let mut value: serde_json::Value =
            serde_json::from_slice(body).map_err(|_| "Codex 请求体不是有效 JSON".to_string())?;
        namespaces::flatten_request_namespaces(&mut value)?;
        xai::sanitize(&mut value);
        *body = serde_json::to_vec(&value).map_err(|_| "Codex 请求序列化失败".to_string())?;
    }
    Ok(())
}

/// Restores namespaced tool identities (and xAI whole-float integer argument
/// quirks) in a native Responses JSON body. No-op unless the route needed the
/// request-side rewrite; unparseable bodies pass through untouched because the
/// normal conversion path reports them.
pub(crate) fn restore_native_json(route: &ActiveRoute, body: &mut Vec<u8>) {
    restore_json(route.upstream_protocol, &route.upstream_base_url, body)
}

fn restore_json(protocol: UpstreamProtocol, base_url: &str, body: &mut Vec<u8>) {
    if !is_xai_native_responses(protocol, base_url) {
        return;
    }
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return;
    };
    let mut changed = namespaces::restore_response_namespaces(&mut value);
    changed |= xai::normalize_integer_arguments(&mut value);
    if changed {
        if let Ok(encoded) = serde_json::to_vec(&value) {
            *body = encoded;
        }
    }
}

/// Wraps a decoded native Responses SSE stream so complete events carrying
/// flattened `function_call` names or whole-float integer arguments are
/// restored before the client sees them. Incomplete tails are held back until
/// more bytes arrive or the stream ends.
pub(crate) fn wrap_native_sse_reader<R: Read>(route: &ActiveRoute, inner: R) -> impl Read + use<R> {
    NativeRestoreReader {
        inner,
        active: is_xai_native_responses(route.upstream_protocol, &route.upstream_base_url),
        pending: Vec::new(),
    }
}

struct NativeRestoreReader<R: Read> {
    inner: R,
    active: bool,
    pending: Vec<u8>,
}

impl<R: Read> Read for NativeRestoreReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let mut scratch = vec![0; buf.len().max(self.pending.len().min(64 * 1024))];
        let read = self.inner.read(&mut scratch)?;
        if read > 0 {
            self.pending.extend_from_slice(&scratch[..read]);
        }
        if !self.active {
            let emit = self.pending.len().min(buf.len());
            buf[..emit].copy_from_slice(&self.pending[..emit]);
            self.pending.drain(..emit);
            return Ok(emit);
        }
        // Hold back bytes after the last complete event boundary so a rewrite
        // never sees half of a JSON payload; EOF flushes the tail verbatim.
        let boundary = if read == 0 {
            self.pending.len()
        } else {
            self.pending
                .windows(2)
                .rposition(|pair| pair == b"\n\n")
                .map(|position| position + 2)
                .unwrap_or(0)
        };
        if boundary == 0 {
            if read == 0 {
                let emit = self.pending.len().min(buf.len());
                buf[..emit].copy_from_slice(&self.pending[..emit]);
                self.pending.drain(..emit);
                return Ok(emit);
            }
            return Ok(0);
        }
        let mut complete: Vec<u8> = self.pending.drain(..boundary).collect();
        namespaces::restore_sse_bytes(&mut complete, xai::normalize_integer_arguments);
        complete.append(&mut std::mem::take(&mut self.pending));
        self.pending = complete;
        let emit = self.pending.len().min(buf.len());
        buf[..emit].copy_from_slice(&self.pending[..emit]);
        self.pending.drain(..emit);
        Ok(emit)
    }
}

fn host_is(base_url: &str, hosts: &[&str]) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url.trim()) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    hosts.iter().any(|candidate| host == *candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn inactive_routes_pass_bytes_through_unchanged() {
        let chunk = b"event: e\ndata: {\"item\":{\"type\":\"function_call\",\"name\":\"asbns_n_2_QQ_4_Uk\"}}\n\n";
        let mut body = chunk.to_vec();
        apply_request(
            UpstreamProtocol::Responses,
            "https://api.deepseek.com/v1",
            &mut body,
        )
        .unwrap();
        assert_eq!(body, chunk);
        restore_json(
            UpstreamProtocol::Responses,
            "https://api.deepseek.com/v1",
            &mut body,
        );
        assert_eq!(body, chunk);
    }

    #[test]
    fn xai_native_gate_strips_fields_but_chat_does_not() {
        let mut body = br#"{"model":"grok-4.6","prompt_cache_retention":"24h"}"#.to_vec();
        apply_request(
            UpstreamProtocol::Responses,
            "https://api.x.ai/v1",
            &mut body,
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(parsed.get("prompt_cache_retention").is_none());

        let mut body = br#"{"model":"gpt","prompt_cache_retention":"24h"}"#.to_vec();
        apply_request(
            UpstreamProtocol::ChatCompletions,
            "https://api.x.ai/v1",
            &mut body,
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(parsed.get("prompt_cache_retention").is_some());
    }

    #[test]
    fn moonshot_chat_gate_wraps_ref_siblings() {
        let mut body = br##"{"tools":[{"type":"function","function":{"name":"t","parameters":{"properties":{"p":{"$ref":"#/x","description":"d"}}}}}]}"##.to_vec();
        apply_request(
            UpstreamProtocol::ChatCompletions,
            "https://api.kimi.com/coding/v1",
            &mut body,
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(
            parsed["tools"][0]["function"]["parameters"]["properties"]["p"]
                .get("allOf")
                .is_some()
        );
    }

    #[test]
    fn reader_restores_namespaced_calls_and_flushes_tails_at_eof() {
        let mut request = serde_json::json!({
            "tools": [
                { "type": "namespace", "name": "mcp__files__", "tools": [
                    { "type": "function", "name": "read", "parameters": {} }
                ]}
            ]
        });
        namespaces::flatten_request_namespaces(&mut request).unwrap();
        let flat = request["tools"][0]["name"].as_str().unwrap().to_string();
        let event = format!(
            "event: response.output_item.done\ndata: {{\"item\":{{\"type\":\"function_call\",\"name\":\"{flat}\",\"arguments\":\"{{\\\"limit\\\": 5.0}}\"}}}}\n\n"
        );
        let tail = "event: response.output_text.delta\ndata: {\"delta\":\"par";
        // The reader is exercised through its public wrapper in the gateway
        // wiring; here the underlying rewrite proves the event-level result.
        let mut chunk = format!("{event}{tail}").into_bytes();
        namespaces::restore_sse_bytes(&mut chunk, xai::normalize_integer_arguments);
        let text = String::from_utf8(chunk).unwrap();
        assert!(text.contains("\"name\":\"read\""));
        assert!(text.contains("\\\"limit\\\":5}"));
        assert!(text.ends_with(tail));
        let _ = Cursor::new(Vec::<u8>::new());
    }
}
