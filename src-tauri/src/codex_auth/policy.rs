use serde::{Deserialize, Serialize};
use std::path::Path;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AuthPolicy {
    pub version: u8,
    pub preserve_official_login: bool,
}
impl Default for AuthPolicy {
    fn default() -> Self {
        Self {
            version: 1,
            preserve_official_login: true,
        }
    }
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AuthPolicyView {
    pub policy: AuthPolicy,
    pub revision: String,
}
fn path(root: &Path) -> std::path::PathBuf {
    root.join("codex/auth-policy.json")
}
pub(crate) fn load(root: &Path) -> Result<AuthPolicyView, String> {
    let raw =
        crate::config_store::read_optional(&path(root)).map_err(|_| "Codex 认证策略不可读")?;
    let policy: AuthPolicy = match &raw {
        None => AuthPolicy::default(),
        Some(raw) => serde_json::from_str(raw).map_err(|_| "Codex 认证策略格式无效")?,
    };
    if policy.version != 1 {
        return Err("Codex 认证策略版本不受支持".into());
    }
    Ok(AuthPolicyView {
        policy,
        revision: asb_switch::sha256_hex(raw.as_deref().unwrap_or("")),
    })
}
pub(crate) fn save(root: &Path, preserve: bool, expected: &str) -> Result<AuthPolicyView, String> {
    let _guard = super::lock()?;
    if load(root)?.revision != expected {
        return Err("Codex 认证策略已变化，请重新读取".into());
    }
    let policy = AuthPolicy {
        version: 1,
        preserve_official_login: preserve,
    };
    crate::config_store::write_json_atomic(
        &path(root),
        &serde_json::to_string_pretty(&policy).map_err(|_| "Codex 认证策略无法序列化")?,
    )?;
    load(root)
}
