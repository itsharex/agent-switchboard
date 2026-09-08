//! Identity and fingerprint helpers for gateway capability tokens.

use super::*;

pub(super) fn is_direct(profile: &ProviderProfile) -> bool {
    !profile.requires_gateway()
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
        "responsesOptions": profile.responses_options,
        "maxOutputTokens": profile.max_output_tokens.value(),
    });
    let bytes = serde_json::to_vec(&payload).map_err(|_| "无法计算供应商路由指纹".to_string())?;
    Ok(hex_digest(&bytes))
}

pub(super) fn route_token(identity: &str, profile_id: &str, fingerprint: &str) -> String {
    format!(
        "asb_local_{}",
        hex_digest(format!("asb/route/v2:{identity}:{profile_id}:{fingerprint}").as_bytes())
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

pub(super) fn continuation_key(identity: &str, profile: &ProviderProfile) -> [u8; 32] {
    let domain = serde_json::json!([
        "asb/continuation/v2",
        identity,
        profile.id,
        profile.app,
        profile.base_url,
        profile.upstream_protocol
    ]);
    Sha256::digest(domain.to_string().as_bytes()).into()
}

impl ActiveRoute {
    pub(crate) fn client_endpoint(&self, gateway_base: &str) -> String {
        match self.app {
            AppKind::Codex => format!("{gateway_base}/codex/{}/v1", self.client_token),
            AppKind::Claude => gateway_base.to_string(),
        }
    }
}
