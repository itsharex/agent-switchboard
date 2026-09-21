//! The configuration snapshot and fingerprint one probe batch runs against.
//! The snapshot is taken before the first call and the fingerprint is
//! checked before, during and after every call. Changed runs never grade.

use super::ledger::ProbeConfigRecord;
use crate::local_state::LocalState;
use asb_core::AppKind;
use sha2::{Digest, Sha256};

/// Reads the effective client configuration and identifies the profile it
/// belongs to. An unidentified configuration keeps `profile_*` empty — the
/// history shows 未关联档案 rather than a guess.
pub(crate) fn capture(
    state: &LocalState,
    gateway: Option<&crate::gateway::GatewayController>,
) -> Result<ProbeConfigRecord, String> {
    let before = fingerprint(state, gateway)?;
    let config_path = state.target(AppKind::Codex)?;
    let text = match std::fs::read_to_string(&config_path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return Err("无法读取当前 Codex 配置".to_string()),
    };
    let doc = text.parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("无法解析当前 Codex 配置：{error}"))?;
    let identified = identify(state, gateway, &text);
    let connection_identity = asb_core::adapter::route_state(AppKind::Codex, &text).base_url
        .and_then(|url| reqwest::Url::parse(&url).ok())
        .map(|mut url| {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            url.to_string()
        });
    let after = fingerprint(state, gateway)?;
    if before != after { return Err("读取快照期间配置变化，请重新开始检测".into()); }
    Ok(ProbeConfigRecord {
        profile_id: identified.as_ref().map(|(id, _)| id.clone()),
        profile_name: identified.as_ref().map(|(_, name)| name.clone()),
        profile_model: doc.get("model").and_then(|value| value.as_str()).map(str::to_owned),
        reasoning_effort: doc.get("model_reasoning_effort").and_then(|value| value.as_str()).map(str::to_owned),
        connection_identity,
        fingerprint: after,
    })
}

/// SHA-256 over the files that define the effective Codex route: the client
/// config (which carries the gateway endpoint, model catalog, and every
/// projected setting) plus the auth file. A hash, never the credentials
/// themselves, is what gets stored.
pub(crate) fn fingerprint(state: &LocalState, gateway: Option<&crate::gateway::GatewayController>) -> Result<String, String> {
    let config_path = state.target(AppKind::Codex)?;
    let auth_path = config_path.with_file_name("auth.json");
    let mut hasher = Sha256::new();
    hasher.update(read_bytes(&config_path)?);
    if let Ok(text) = std::fs::read_to_string(&config_path) {
        let doc = text.parse::<toml_edit::DocumentMut>().map_err(|error| error.to_string())?;
        if let Some(path) = doc.get("model_catalog_json").and_then(|value| value.as_str()) {
            let path = std::path::PathBuf::from(path);
            let path = if path.is_absolute() { path } else {
                config_path.parent().ok_or("Codex 配置路径无效")?.join(path)
            };
            hasher.update(read_bytes(&path)?);
        }
    }
    hasher.update([0u8]);
    hasher.update(read_bytes(&auth_path)?);
    if let Some(gateway) = gateway {
        let text = match std::fs::read_to_string(&config_path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(format!("无法校验网关配置：{error}")),
        };
        hasher.update(gateway.probe_route_revision(&text)?);
    }
    Ok(hex(&hasher.finalize()))
}

/// The pre-run check: `Ok(false)` means the effective configuration changed
/// since the batch started.
pub(crate) fn matches(state: &LocalState, gateway: Option<&crate::gateway::GatewayController>, expected: &str) -> Result<bool, String> {
    Ok(fingerprint(state, gateway)? == expected)
}

/// Gateway-routed profiles first, then a direct activation match; anything
/// else (official login, hand-edited config, ambiguous match, unreadable
/// store) is unlinked rather than a guess. Identification failures never
/// block a probe — only persistence failures may.
fn identify(
    state: &LocalState,
    gateway: Option<&crate::gateway::GatewayController>,
    text: &str,
) -> Option<(String, String)> {
    if let Some(gateway) = gateway {
        match gateway.active_profile_id(AppKind::Codex, text) {
            Ok(Some(id)) => {
                if let Some(name) = profile_name(state, &id) {
                    return Some((id, name));
                }
            }
            Ok(None) => {}
            Err(error) => log::warn!("无法确认网关激活的 Codex 档案：{error}"),
        }
    }
    match crate::commands::status::codex::active_direct(state, text) {
        Ok(Some(id)) => {
            if let Some(name) = profile_name(state, &id) {
                return Some((id, name));
            }
        }
        Ok(None) => {}
        Err(error) => log::warn!("无法识别直连激活的 Codex 档案：{}", error.message),
    }
    None
}

fn profile_name(state: &LocalState, id: &str) -> Option<String> {
    state
        .configuration()
        .find_codex_provider_file(id)
        .ok()
        .map(|file| file.profile.name)
}

fn read_bytes(path: &std::path::Path) -> Result<Vec<u8>, String> {
    match std::fs::read(path) {
        Ok(bytes) => { let mut marked = vec![1]; marked.extend(bytes); Ok(marked) }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(vec![0]),
        Err(error) => Err(format!("无法读取检测配置 {}：{error}", path.display())),
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
