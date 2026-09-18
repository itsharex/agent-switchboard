use super::{contracts::{Account, Tokens}, identity, oauth, store};
use std::{fs, path::Path};

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
