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
