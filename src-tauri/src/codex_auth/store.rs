use super::contracts::{AccountSelection, AccountSummary, AccountsFile, AccountsView};
use asb_switch::{io::FsIo, sha256_hex, SwitchIo};
use std::{
    collections::HashSet,
    fs,
    path::Path,
    sync::{Mutex, MutexGuard, OnceLock},
};

pub(crate) fn lock() -> Result<MutexGuard<'static, ()>, String> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Codex 账号锁不可用，请重启应用".into())
}
pub(super) fn load(root: &Path) -> Result<(AccountsFile, String), String> {
    let path = root.join("codex/accounts.json");
    let raw = match fs::read_to_string(path) {
        Ok(value) => Some(value),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err("无法读取 Codex 账号库，请检查文件权限".into()),
    };
    let file: AccountsFile = if raw.is_none() {
        AccountsFile::default()
    } else {
        serde_json::from_str(raw.as_deref().unwrap())
            .map_err(|_| "Codex 账号库格式无效，请从备份恢复")?
    };
    validate(&file)?;
    Ok((file, sha256_hex(raw.as_deref().unwrap_or(""))))
}
fn validate(file: &AccountsFile) -> Result<(), String> {
    if file.version != 1 {
        return Err("Codex 账号库版本不受支持".into());
    }
    let mut ids = HashSet::new();
    let mut identities = HashSet::new();
    for account in &file.accounts {
        if let Some(sync) = &account.native_sync {
            if [&sync.content_hash, &sync.refresh_hash]
                .iter()
                .any(|hash| hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
            {
                return Err("Codex 原生认证同步版本无效".into());
            }
        }
        if uuid::Uuid::parse_str(&account.id).is_err()
            || !ids.insert(&account.id)
            || !identities.insert((&account.identity.subject, &account.identity.account_id))
            || !account
                .identity
                .same_user(&super::identity::identity(&account.tokens)?)
            || account.generation == 0
        {
            return Err("Codex 账号库包含重复或无效身份".into());
        }
    }
    if file.default_id.as_ref().is_some_and(|id| !ids.contains(id)) {
        return Err("Codex 默认账号不存在".into());
    }
    for (profile, selection) in &file.bindings {
        if uuid::Uuid::parse_str(profile).is_err() {
            return Err("Codex 账号绑定的档案标识无效".into());
        }
        if let AccountSelection::Account { id } = selection {
            if !ids.contains(id) {
                return Err("Codex 档案绑定的账号不存在".into());
            }
        }
    }
    Ok(())
}
pub(super) fn save(root: &Path, file: &AccountsFile, expected: &str) -> Result<String, String> {
    validate(file)?;
    if load(root)?.1 != expected {
        return Err("Codex 账号库已变化，请重新读取".into());
    }
    let text = serde_json::to_string_pretty(file).map_err(|_| "Codex 账号库无法序列化")?;
    let path = root.join("codex/accounts.json");
    let parent = path.parent().ok_or("Codex 账号库路径无效")?;
    fs::create_dir_all(parent).map_err(|_| "无法创建 Codex 账号目录")?;
    let temporary = parent.join(format!("accounts.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        FsIo.write_new_file(&temporary, &text)
            .map_err(|_| "无法写入 Codex 账号临时文件")?;
        FsIo.set_mode(&temporary, 0o600)
            .map_err(|_| "无法限制 Codex 账号文件权限")?;
        FsIo.sync_file(&temporary)
            .map_err(|_| "无法同步 Codex 账号文件")?;
        fs::rename(&temporary, &path).map_err(|_| "无法原子替换 Codex 账号库")?;
        FsIo.sync_dir(parent)
            .map_err(|_| "Codex 账号库已保存，但目录同步失败")?;
        Ok::<_, String>(sha256_hex(&text))
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
pub(super) fn view(file: &AccountsFile, revision: String) -> AccountsView {
    let accounts = file
        .accounts
        .iter()
        .map(|account| AccountSummary {
            id: account.id.clone(),
            email: account.identity.email.clone(),
            plan: account.identity.plan.clone(),
            account_label: sha256_hex(&format!(
                "{}:{}",
                account.identity.subject, account.identity.account_id
            ))[..12]
                .into(),
            is_default: file.default_id.as_ref() == Some(&account.id),
            generation: account.generation,
            expires_at: account.expires_at,
            native_sync_pending: account.native_sync.is_some(),
            native_sync_error: account.native_sync_error.clone(),
            bound_profile_ids: file
                .bindings
                .iter()
                .filter_map(|(id, binding)| match binding {
                    AccountSelection::Account { id: bound } if bound == &account.id => {
                        Some(id.clone())
                    }
                    AccountSelection::Default if file.default_id.as_ref() == Some(&account.id) => {
                        Some(id.clone())
                    }
                    _ => None,
                })
                .collect(),
        })
        .collect();
    AccountsView {
        revision,
        accounts,
        bindings: file.bindings.clone(),
    }
}
