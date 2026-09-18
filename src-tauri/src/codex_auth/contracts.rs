use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum AccountSelection {
    #[default]
    Native,
    Default,
    Account {
        id: String,
    },
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Tokens {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
}
impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CodexTokens([redacted])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Identity {
    pub subject: String,
    pub account_id: String,
    pub email: Option<String>,
    pub plan: Option<String>,
}
impl Identity {
    pub fn same_user(&self, other: &Self) -> bool {
        self.subject == other.subject && self.account_id == other.account_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct NativeSync {
    pub content_hash: String,
    pub refresh_hash: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Account {
    pub id: String,
    pub identity: Identity,
    pub tokens: Tokens,
    pub generation: u64,
    pub updated_at: i64,
    pub expires_at: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) native_sync: Option<NativeSync>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub native_sync_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AccountsFile {
    pub version: u8,
    pub default_id: Option<String>,
    pub accounts: Vec<Account>,
    pub bindings: BTreeMap<String, AccountSelection>,
}
impl Default for AccountsFile {
    fn default() -> Self {
        Self {
            version: 1,
            default_id: None,
            accounts: Vec::new(),
            bindings: BTreeMap::new(),
        }
    }
}



