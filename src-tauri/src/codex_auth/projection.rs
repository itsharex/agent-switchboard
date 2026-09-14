use super::{binding, contracts::AccountSelection, valid_account};
use asb_core::{contracts::CodexManagedAuth, AppKind, SwitchPlan};
use std::path::Path;

pub(crate) fn official_plan(
    state: &crate::local_state::LocalState,
    plan: SwitchPlan,
) -> Result<SwitchPlan, String> {
    let target = state.target(AppKind::Codex)?;
    let selection = binding(state.root(), &plan.profile.id)?;
    let id = match &selection {
        AccountSelection::Native => {
            require_native(&target)?;
            return Ok(plan);
        }
        AccountSelection::Default => None,
        AccountSelection::Account { id } => Some(id.as_str()),
    };
    let account = valid_account(state.root(), id, &target.with_file_name("auth.json"))?;
    Ok(plan.with_codex_managed_auth(for_account(&account)?))
}

pub(super) fn for_account(account: &super::contracts::Account) -> Result<CodexManagedAuth, String> {
    let last_refresh = chrono::DateTime::from_timestamp_millis(account.updated_at)
        .ok_or("Codex 账号刷新时间无效")?
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    Ok(CodexManagedAuth {
        managed_id: account.id.clone(),
        account_id: account.identity.account_id.clone(),
        subject: account.identity.subject.clone(),
        id_token: account.tokens.id_token.clone(),
        access_token: account.tokens.access_token.clone(),
        refresh_token: account.tokens.refresh_token.clone(),
        generation: account.generation,
        last_refresh,
    })
}

pub(crate) fn require_native(target: &Path) -> Result<(), String> {
    use crate::official_login::observation::{
        observe_codex_login, parse_codex_login, CodexLoginState,
    };
    let observed = observe_codex_login(target);
    if observed == CodexLoginState::Configured {
        return Ok(());
    }
    if observed != CodexLoginState::ApiKey {
        return observed.require();
    }
    // An API-key switch can preserve the previous OAuth bundle. Switching
    // back is valid even though the current native auth mode is API key.
    let text = std::fs::read_to_string(target.with_file_name("auth.json"))
        .map_err(|_| "无法读取 Codex 官方登录缓存")?;
    let mut value: serde_json::Value =
        serde_json::from_str(&text).map_err(|_| "Codex 登录缓存格式无效")?;
    value["OPENAI_API_KEY"] = serde_json::Value::Null;
    value["auth_mode"] = serde_json::Value::String("chatgpt".into());
    parse_codex_login(&value.to_string()).require()
}

/// Kept through the executor commit so a concurrent refresh or deletion cannot
/// install an older token bundle after a newer account generation was saved.
pub(crate) fn commit_guard(
    root: &Path,
    profile_id: &str,
    auth: &CodexManagedAuth,
) -> Result<std::sync::MutexGuard<'static, ()>, String> {
    let guard = super::lock()?;
    let (file, _) = super::store::load(root)?;
    let selected = match file.bindings.get(profile_id) {
        Some(AccountSelection::Account { id }) => Some(id.as_str()),
        Some(AccountSelection::Default) => file.default_id.as_deref(),
        _ => None,
    };
    let current = file
        .accounts
        .iter()
        .find(|a| a.id == auth.managed_id)
        .ok_or("Codex 托管账号已被删除")?;
    if selected != Some(auth.managed_id.as_str()) || current.generation != auth.generation {
        return Err("Codex 账号绑定或令牌代际已变化，请重新预览后切换".into());
    }
    Ok(guard)
}

pub(crate) fn matches_binding(
    state: &crate::local_state::LocalState,
    profile_id: &str,
) -> Result<bool, String> {
    let _guard = super::lock()?;
    let (file, _) = super::store::load(state.root())?;
    let id = match file.bindings.get(profile_id) {
        None | Some(AccountSelection::Native) => return Ok(true),
        Some(AccountSelection::Default) => file.default_id.as_deref(),
        Some(AccountSelection::Account { id }) => Some(id.as_str()),
    };
    let Some(account) = file.accounts.iter().find(|a| Some(a.id.as_str()) == id) else {
        return Ok(false);
    };
    let path = state.target(AppKind::Codex)?.with_file_name("auth.json");
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err("无法读取 Codex 身份状态".into()),
    };
    Ok(super::identity::native(&text)
        .ok()
        .and_then(|(tokens, _)| super::identity::identity(&tokens).ok())
        .is_some_and(|identity| account.identity.same_user(&identity)))
}

pub(crate) fn validate_backup(root: &Path, content: &str) -> Result<(), String> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(content) else {
        return Ok(());
    };
    let Some(marker) = value.get("asb_managed_account") else {
        return Ok(());
    };
    let id = marker
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or("Codex 托管认证备份缺少账号标识")?;
    let generation = marker
        .get("generation")
        .and_then(|v| v.as_u64())
        .ok_or("Codex 托管认证备份缺少令牌代际")?;
    let _guard = super::lock()?;
    let (file, _) = super::store::load(root)?;
    let account = file
        .accounts
        .iter()
        .find(|a| a.id == id)
        .ok_or("此备份引用的 Codex 托管账号已被删除，不能恢复旧登录")?;
    if account.generation > generation {
        return Err("此备份的 Codex 令牌已过时，请重新应用账号绑定，不要恢复旧令牌".into());
    }
    Ok(())
}
