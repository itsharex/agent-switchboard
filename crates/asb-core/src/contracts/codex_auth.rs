//! Private executor input, never serialized across the desktop boundary.
#[derive(Clone, PartialEq)]
pub struct CodexManagedAuth {
    pub managed_id: String,
    pub account_id: String,
    pub subject: String,
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    pub generation: u64,
    pub last_refresh: String,
}
impl std::fmt::Debug for CodexManagedAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CodexManagedAuth([redacted])")
    }
}
