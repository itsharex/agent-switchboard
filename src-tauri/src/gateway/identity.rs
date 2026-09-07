//! Identity and fingerprint helpers for gateway capability tokens.

use super::*;

pub(super) fn is_direct(profile: &ProviderProfile) -> bool {
    let Some(protocol) = profile.upstream_protocol else {
        return false;
    };
    protocol == UpstreamProtocol::native_for(profile.app)
}

/// Route fingerprint over the exact profile parameters a loopback route
/// consumes. Application-side metadata (name, notes, website, usage query)
/// and client-side projection fields (model, model options) must not rotate
/// the capability token: editing them never invalidates a live route.
pub(super) fn route_fingerprint(profile: &ProviderProfile) -> Result<String, String> {
    let payload = serde_json::json!({
        "app": profile.app,
        "baseUrl": profile.base_url,
        "apiKey": profile.api_key,
        "upstreamProtocol": profile.upstream_protocol,
        "maxOutputTokens": profile.max_output_tokens.value(),
    });
    let bytes = serde_json::to_vec(&payload).map_err(|_| "无法计算供应商路由指纹".to_string())?;
    Ok(hex_digest(&bytes))
}

pub(super) fn route_token(identity: &str, fingerprint: &str) -> String {
    format!(
        "asb_local_{}",
        hex_digest(format!("{identity}:{fingerprint}").as_bytes())
    )
}

pub(super) fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn constant_time_equal(expected: &[u8], received: &[u8]) -> bool {
    let mut different = expected.len() ^ received.len();
    for index in 0..expected.len().max(received.len()) {
        different |=
            usize::from(*expected.get(index).unwrap_or(&0) ^ *received.get(index).unwrap_or(&0));
    }
    different == 0
}
