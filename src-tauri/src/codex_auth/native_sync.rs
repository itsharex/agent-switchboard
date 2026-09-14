//! A shared refresh-token chain must keep its native copy usable. Only the
//! executor writes auth.json; app storage retains an interrupted sync intent.
use super::{
    contracts::{Account, NativeSync},
    identity, projection, store,
};
use std::path::Path;
pub(super) fn capture(account: &Account, auth_path: &Path) -> Result<Option<NativeSync>, String> {
    let text = match std::fs::read_to_string(auth_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("无法检查原生凭据是否共享此刷新令牌".into()),
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Ok(None);
    };
    if !value.is_object() {
        return Ok(None);
    }
    // An API-key route may retain OAuth for later use. Refresh its token
    // bundle without changing the currently selected API-key identity.
    value["OPENAI_API_KEY"] = serde_json::Value::Null;
    let Ok((tokens, _)) = identity::native(&value.to_string()) else {
        return Ok(None);
    };
    if tokens.refresh_token != account.tokens.refresh_token
        || !account.identity.same_user(&identity::identity(&tokens)?)
    {
        return Ok(None);
    }
    require_file_storage(auth_path)?;
    Ok(Some(NativeSync {
        content_hash: asb_switch::sha256_hex(&text),
        refresh_hash: asb_switch::sha256_hex(&tokens.refresh_token),
    }))
}
pub(super) fn require_file_storage(auth_path: &Path) -> Result<(), String> {
    let config = match std::fs::read_to_string(auth_path.with_file_name("config.toml")) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("无法检查 Codex 认证存储方式".into()),
    };
    let document = config
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "Codex 配置格式无效")?;
    if document
        .get("cli_auth_credentials_store")
        .is_some_and(|v| v.as_str() != Some("file"))
    {
        return Err("此登录与原生凭据共享刷新令牌，但原生存储不是 file；请先明确选择 file 或重新添加独立托管登录".into());
    }
    Ok(())
}
pub(super) fn finish(root: &Path, id: &str, auth_path: &Path) -> Result<Account, String> {
    let (mut file, revision) = store::load(root)?;
    let account = file
        .accounts
        .iter_mut()
        .find(|a| a.id == id)
        .ok_or("Codex 账号已被删除")?;
    let Some(sync) = account.native_sync.clone() else {
        return Ok(account.clone());
    };
    let previous = (
        account.native_sync.clone(),
        account.native_sync_error.clone(),
    );
    let outcome = match require_file_storage(auth_path) {
        Ok(()) => asb_switch::synchronize_codex_auth(
            &asb_switch::FsIo,
            &auth_path.with_file_name("config.toml"),
            &root.join("codex/auth-backups"),
            &sync.content_hash,
            &sync.refresh_hash,
            &projection::for_account(account)?,
        )
        .map_err(|error| error.to_string()),
        Err(error) => Err(error),
    };
    match outcome {
        Ok(_) => {
            account.native_sync = None;
            account.native_sync_error = None;
        }
        Err(error) => {
            account.native_sync_error = Some(format!(
                "账号已刷新，但原生认证同步未完成：{error}；可重新应用该账号恢复"
            ))
        }
    }
    let changed = previous
        != (
            account.native_sync.clone(),
            account.native_sync_error.clone(),
        );
    let result = account.clone();
    if changed {
        store::save(root, &file, &revision)?;
    }
    Ok(result)
}
