//! Claude-only local managed accounts and request-time credential resolution.
//! Accounts live in application state, never in Codex auth.json or Claude .credentials.json.

mod contracts;
mod copilot;
pub(crate) mod copilot_model;
pub(crate) mod device;
mod http;
pub(crate) mod models;
mod oauth;
pub(crate) mod quota;
mod refresh;
pub(crate) mod request;
mod store;
#[cfg(test)]
mod tests;

use asb_core::{
    claude_auth::{managed_auth, ClaudeAuthProvider},
    contracts::ProviderConnectionOptions,
};
pub(crate) use contracts::{ClaudeAccount, ClaudeAccountsView};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

#[derive(Clone)]
pub(crate) struct ResolvedAccount {
    pub provider: ClaudeAuthProvider,
    pub account_id: String,
    pub access_token: String,
    pub expires_at_ms: i64,
    pub endpoint: String,
    pub upstream_account_id: Option<String>,
}

#[derive(Default)]
struct Runtime {
    pending_refresh: BTreeMap<String, refresh::PendingRefresh>,
    sessions: BTreeMap<String, device::Session>,
    copilot: BTreeMap<String, (String, ResolvedAccount)>,
}

pub(crate) struct ClaudeAuth {
    root: PathBuf,
    runtime: Mutex<Runtime>,
    #[cfg(test)]
    endpoints: Option<TestEndpoints>,
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct TestEndpoints {
    pub github_api: String,
    pub oauth_token: String,
    pub upstream: String,
}

impl ClaudeAuth {
    pub(crate) fn shared(root: &Path) -> Arc<Self> {
        let mut instances = instances()
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        instances
            .entry(root.to_path_buf())
            .or_insert_with(|| Arc::new(Self::new(root)))
            .clone()
    }

    #[cfg(test)]
    pub(crate) fn register_test_endpoints(root: &Path, endpoints: TestEndpoints) -> Arc<Self> {
        let auth = Arc::new(Self::with_endpoints(root, endpoints));
        instances()
            .lock()
            .unwrap()
            .insert(root.to_path_buf(), auth.clone());
        auth
    }

    pub(crate) fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            runtime: Mutex::new(Runtime::default()),
            #[cfg(test)]
            endpoints: None,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_endpoints(root: &Path, endpoints: TestEndpoints) -> Self {
        Self {
            endpoints: Some(endpoints),
            ..Self::new(root)
        }
    }

    pub(crate) fn view(&self) -> Result<ClaudeAccountsView, String> {
        let (file, hash) = store::load(&self.root)?;
        Ok(file.view(hash))
    }

    pub(crate) fn save_account(
        &self,
        account: ClaudeAccount,
        expected_hash: &str,
        make_default: bool,
    ) -> Result<ClaudeAccountsView, String> {
        account.validate()?;
        let mut runtime = self.runtime.lock().map_err(|_| "Claude 认证写入锁不可用")?;
        let (mut file, _) = store::load(&self.root)?;
        if file
            .accounts
            .iter()
            .any(|saved| saved.id == account.id && saved.provider != account.provider)
        {
            return Err("不能将已有 Claude 托管账号 ID 改为其他认证服务".into());
        }
        let account_id = account.id.clone();
        if make_default {
            file.defaults.insert(account.provider, account.id.clone());
        }
        if let Some(saved) = file
            .accounts
            .iter_mut()
            .find(|saved| saved.id == account.id)
        {
            *saved = account;
        } else {
            file.accounts.push(account);
        }
        let hash = store::save(&self.root, &file, expected_hash)?;
        runtime.copilot.remove(&account_id);
        runtime.pending_refresh.remove(&account_id);
        Ok(file.view(hash))
    }

    pub(crate) fn set_default(
        &self,
        provider: ClaudeAuthProvider,
        account_id: &str,
        expected_hash: &str,
    ) -> Result<ClaudeAccountsView, String> {
        let _guard = self.runtime.lock().map_err(|_| "Claude 认证写入锁不可用")?;
        let (mut file, _) = store::load(&self.root)?;
        file.defaults.insert(provider, account_id.into());
        let hash = store::save(&self.root, &file, expected_hash)?;
        Ok(file.view(hash))
    }

    pub(crate) fn remove(
        &self,
        account_id: &str,
        expected_hash: &str,
    ) -> Result<ClaudeAccountsView, String> {
        let mut runtime = self.runtime.lock().map_err(|_| "Claude 认证写入锁不可用")?;
        let (mut file, _) = store::load(&self.root)?;
        if !file.accounts.iter().any(|account| account.id == account_id) {
            return Err("Claude 托管账号不存在".into());
        }
        file.accounts.retain(|account| account.id != account_id);
        file.defaults.retain(|_, id| id != account_id);
        let hash = store::save(&self.root, &file, expected_hash)?;
        runtime.copilot.remove(account_id);
        runtime.pending_refresh.remove(account_id);
        Ok(file.view(hash))
    }

    pub(crate) fn resolve(
        &self,
        connection: &ProviderConnectionOptions,
    ) -> Result<Option<ResolvedAccount>, String> {
        let Some((provider, selected)) = managed_auth(connection)? else {
            return Ok(None);
        };
        let mut runtime = self.runtime.lock().map_err(|_| "Claude 认证锁不可用")?;
        let (mut file, hash) = store::load(&self.root)?;
        let id = selected
            .or_else(|| file.defaults.get(&provider).map(String::as_str))
            .ok_or("此 Claude 认证服务没有默认账号，请登录或指定托管账号")?;
        let index = file
            .accounts
            .iter()
            .position(|account| account.id == id && account.provider == provider)
            .ok_or("Claude 绑定的托管账号不存在或认证服务不匹配；不会回退到其他账号")?;
        let account = &file.accounts[index];
        let now = chrono::Utc::now().timestamp_millis();
        let resolved = if provider == ClaudeAuthProvider::GithubCopilot {
            self.resolve_copilot(&mut runtime, account, now)?
        } else {
            refresh::resolve(self, &mut runtime, &mut file, index, &hash, now)?;
            let account = &file.accounts[index];
            ResolvedAccount {
                provider,
                account_id: account.id.clone(),
                access_token: account.access_token.clone(),
                expires_at_ms: account.expires_at_ms.unwrap_or(0),
                endpoint: provider.default_endpoint().into(),
                upstream_account_id: account.upstream_account_id.clone(),
            }
        };
        #[cfg(test)]
        let resolved = if let Some(endpoints) = &self.endpoints {
            ResolvedAccount {
                endpoint: endpoints.upstream.clone(),
                ..resolved
            }
        } else {
            resolved
        };
        Ok(Some(resolved))
    }

    fn resolve_copilot(
        &self,
        runtime: &mut Runtime,
        account: &ClaudeAccount,
        now: i64,
    ) -> Result<ResolvedAccount, String> {
        let serialized = serde_json::to_string(account).map_err(|_| "Claude 账号修订无法计算")?;
        let revision = asb_switch::sha256_hex(&serialized);
        if let Some((cached_revision, cached)) = runtime.copilot.get(&account.id) {
            if cached_revision == &revision && cached.expires_at_ms > now.saturating_add(60_000) {
                return Ok(cached.clone());
            }
        }
        let base = copilot::api_base(account);
        #[cfg(test)]
        let base = self
            .endpoints
            .as_ref()
            .map(|endpoints| endpoints.github_api.clone())
            .unwrap_or(base);
        let resolved = copilot::exchange(account, &base, now)?;
        runtime
            .copilot
            .insert(account.id.clone(), (revision, resolved.clone()));
        Ok(resolved)
    }

    fn token_url(&self, provider: ClaudeAuthProvider) -> Result<String, String> {
        #[cfg(test)]
        if let Some(endpoints) = &self.endpoints {
            return Ok(endpoints.oauth_token.clone());
        }
        match provider {
            ClaudeAuthProvider::CodexOauth => Ok(oauth::CHATGPT_TOKEN_URL.into()),
            ClaudeAuthProvider::XaiOauth => oauth::xai_token_endpoint(oauth::XAI_DISCOVERY_URL),
            ClaudeAuthProvider::GithubCopilot => Err("Copilot 不使用 OAuth 刷新端点".into()),
        }
    }
}

fn instances() -> &'static Mutex<BTreeMap<PathBuf, Arc<ClaudeAuth>>> {
    static INSTANCES: OnceLock<Mutex<BTreeMap<PathBuf, Arc<ClaudeAuth>>>> = OnceLock::new();
    INSTANCES.get_or_init(Default::default)
}
