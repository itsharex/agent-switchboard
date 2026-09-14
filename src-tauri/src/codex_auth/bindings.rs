use super::{
    contracts::{AccountSelection, AccountsView},
    store,
};
use std::path::Path;

pub(crate) fn binding(root: &Path, profile_id: &str) -> Result<AccountSelection, String> {
    let _guard = store::lock()?;
    Ok(store::load(root)?
        .0
        .bindings
        .get(profile_id)
        .cloned()
        .unwrap_or_default())
}
pub(crate) fn set_binding(
    root: &Path,
    profile_id: &str,
    selection: AccountSelection,
    expected: &str,
) -> Result<AccountsView, String> {
    let _guard = store::lock()?;
    let (mut file, _) = store::load(root)?;
    if let AccountSelection::Account { id } = &selection {
        if !file.accounts.iter().any(|a| &a.id == id) {
            return Err("所选 Codex 账号不存在".into());
        }
    }
    if selection == AccountSelection::Default && file.default_id.is_none() {
        return Err("请先指定 Codex 默认账号".into());
    }
    if selection == AccountSelection::Native {
        file.bindings.remove(profile_id);
    } else {
        file.bindings.insert(profile_id.to_string(), selection);
    }
    let revision = store::save(root, &file, expected)?;
    Ok(store::view(&file, revision))
}

pub(crate) fn clear_bindings(root: &Path) -> Result<(), String> {
    let _guard = store::lock()?;
    let (mut file, revision) = store::load(root)?;
    if file.bindings.is_empty() {
        return Ok(());
    }
    file.bindings.clear();
    store::save(root, &file, &revision)?;
    Ok(())
}
pub(crate) fn require_unbound(root: &Path, id: &str) -> Result<(), String> {
    if binding(root, id)? != AccountSelection::Native {
        return Err("请先在 Codex 认证管理中解除此档案的账号绑定，再删除档案".into());
    }
    Ok(())
}
