use super::{
    contracts::{Account, AccountSelection, AccountsView, Tokens},
    identity, oauth, store,
};
use std::{fs, path::Path};

pub(crate) fn list_accounts(root: &Path) -> Result<AccountsView, String> {
    let _guard = store::lock()?;
    let (file, revision) = store::load(root)?;
    Ok(store::view(&file, revision))
}
pub(crate) fn import_native(
    root: &Path,
    auth_path: &Path,
    expected: &str,
) -> Result<AccountsView, String> {
    let _guard = store::lock()?;
    super::native_sync::require_file_storage(auth_path)?;
    let text = fs::read_to_string(auth_path).map_err(|_| "无法读取 Codex 原生登录文件")?;
    let (tokens, updated) = identity::native(&text)?;
    upsert(root, tokens, None, updated, Some(expected))?;
    let (file, revision) = store::load(root)?;
    Ok(store::view(&file, revision))
}
pub(super) fn upsert(
    root: &Path,
    tokens: Tokens,
    reauthenticate: Option<&str>,
    updated_at: i64,
    expected: Option<&str>,
) -> Result<String, String> {
    let (mut file, revision) = store::load(root)?;
    if expected.is_some_and(|value| value != revision) {
        return Err("Codex 账号库已变化，请重新读取".into());
    }
    let identity = identity::identity(&tokens)?;
    let existing = if let Some(id) = reauthenticate {
        let account = file
            .accounts
            .iter()
            .find(|a| a.id == id)
            .ok_or("重认证目标账号已被删除")?;
        if !account.identity.same_user(&identity) {
            return Err("重认证登录的用户或工作区与目标账号不同".into());
        }
        Some(id.to_string())
    } else {
        file.accounts
            .iter()
            .find(|a| a.identity.same_user(&identity))
            .map(|a| a.id.clone())
    };
    let id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let previous = file.accounts.iter().find(|a| a.id == id);
    if reauthenticate.is_none() && previous.is_some_and(|a| a.tokens == tokens) {
        return Ok(id);
    }
    if reauthenticate.is_none() && previous.is_some_and(|a| updated_at < a.updated_at) {
        return Err("原生登录凭据早于托管账号，拒绝回退令牌代际".into());
    }
    let generation = previous.map_or(Ok(1), |a| {
        a.generation.checked_add(1).ok_or("Codex 令牌代际溢出")
    })?;
    let updated_at = updated_at.max(chrono::Utc::now().timestamp_millis());
    let expires_at = identity::expires_at(&tokens, updated_at);
    file.accounts.retain(|a| a.id != id);
    file.accounts.push(Account {
        id: id.clone(),
        identity,
        tokens,
        generation,
        updated_at,
        expires_at,
        native_sync: None,
        native_sync_error: None,
    });
    store::save(root, &file, &revision)?;
    Ok(id)
}
pub(crate) fn set_default(
    root: &Path,
    id: Option<&str>,
    expected: &str,
) -> Result<AccountsView, String> {
    let _guard = store::lock()?;
    let (mut file, _) = store::load(root)?;
    if id.is_some_and(|id| !file.accounts.iter().any(|a| a.id == id)) {
        return Err("Codex 账号不存在".into());
    }
    file.default_id = id.map(str::to_string);
    let revision = store::save(root, &file, expected)?;
    Ok(store::view(&file, revision))
}
pub(crate) fn delete_account(
    root: &Path,
    id: &str,
    expected: &str,
) -> Result<AccountsView, String> {
    let _guard = store::lock()?;
    let (mut file, _) = store::load(root)?;
    if !file.accounts.iter().any(|a| a.id == id) {
        return Err("Codex 账号不存在".into());
    }
    if file.bindings.values().any(|b| {
        matches!(b, AccountSelection::Account { id: bound } if bound == id)
            || matches!(b, AccountSelection::Default) && file.default_id.as_deref() == Some(id)
    }) {
        return Err("请先解绑引用此 Codex 账号的供应商档案，再删除账号".into());
    }
    file.accounts.retain(|a| a.id != id);
    if file.default_id.as_deref() == Some(id) {
        file.default_id = None;
    }
    let revision = store::save(root, &file, expected)?;
    Ok(store::view(&file, revision))
}
pub(crate) fn valid_account(
    root: &Path,
    id: Option<&str>,
    auth_path: &Path,
) -> Result<Account, String> {
    let _guard = store::lock()?;
    valid_account_with(
        root,
        id,
        auth_path,
        chrono::Utc::now().timestamp_millis(),
        oauth::refresh,
    )
}
pub(super) fn valid_account_with(
    root: &Path,
    id: Option<&str>,
    auth_path: &Path,
    now: i64,
    refresh: impl Fn(&Tokens) -> Result<Tokens, String>,
) -> Result<Account, String> {
    let (mut file, revision) = store::load(root)?;
    let id = id
        .or(file.default_id.as_deref())
        .ok_or("尚未指定 Codex 默认账号")?
        .to_string();
    let account = file
        .accounts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or("所选 Codex 账号不存在，请重新绑定")?;
    let adopted = adopt_native(account, auth_path)?;
    if account.expires_at > now.saturating_add(60_000) {
        let account = account.clone();
        if adopted {
            store::save(root, &file, &revision)?;
        }
        return super::native_sync::finish(root, &account.id, auth_path);
    }
    let pending_sync =
        super::native_sync::capture(account, auth_path)?.or_else(|| account.native_sync.clone());
    let tokens = match refresh(&account.tokens) {
        Ok(tokens) => tokens,
        Err(error) => {
            if !adopt_native(account, auth_path)? {
                return Err(error);
            }
            refresh(&account.tokens)?
        }
    };
    if !account.identity.same_user(&identity::identity(&tokens)?) {
        return Err("刷新返回了不同 Codex 身份，已拒绝替换账号".into());
    }
    account.generation = account
        .generation
        .checked_add(1)
        .ok_or("Codex 令牌代际溢出")?;
    account.expires_at = identity::expires_at(&tokens, now);
    account.updated_at = now.max(account.updated_at.saturating_add(1));
    account.tokens = tokens;
    account.native_sync = pending_sync;
    let id = account.id.clone();
    store::save(root, &file, &revision).map_err(|error| {
        format!("Codex 登录已刷新，但持久化失败：{error}；请修复本地存储后重新认证")
    })?;
    super::native_sync::finish(root, &id, auth_path)
}
fn adopt_native(account: &mut Account, auth_path: &Path) -> Result<bool, String> {
    let text = match fs::read_to_string(auth_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err("无法检查 Codex 原生令牌代际，请检查文件权限".into()),
    };
    let Ok((tokens, updated)) = identity::native(&text) else {
        return Ok(false);
    };
    if updated <= account.updated_at || !account.identity.same_user(&identity::identity(&tokens)?) {
        return Ok(false);
    }
    account.expires_at = identity::expires_at(&tokens, updated);
    account.tokens = tokens;
    account.updated_at = updated;
    account.generation = account
        .generation
        .checked_add(1)
        .ok_or("Codex 令牌代际溢出")?;
    Ok(true)
}
