use super::{
    contracts::AccountSelection,
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
