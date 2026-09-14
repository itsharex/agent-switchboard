//! Managed quota reset baselines cannot share the machine-native baseline.
use crate::codex_official_quota::{apply_read, CodexQuotaBaseline};
use asb_core::contracts::{CodexOfficialQuota, CodexOfficialQuotaReset};
use std::{collections::BTreeMap, path::Path, sync::Mutex};
static LOCK: Mutex<()> = Mutex::new(());
pub(super) fn record(
    root: &Path,
    id: &str,
    quota: &CodexOfficialQuota,
) -> Result<Option<CodexOfficialQuotaReset>, String> {
    let _guard = LOCK.lock().map_err(|_| "Codex 账号额度历史锁不可用")?;
    let path = root.join("codex/account-quota-history.json");
    let raw = crate::config_store::read_optional(&path).map_err(|_| "Codex 账号额度历史不可读")?;
    let mut entries: BTreeMap<String, CodexQuotaBaseline> = match raw {
        None => BTreeMap::new(),
        Some(raw) => serde_json::from_str(&raw)
            .map_err(|_| "Codex 账号额度历史格式无效，请保留文件并检查备份")?,
    };
    for baseline in entries.values() {
        baseline.validate()?;
    }
    let previous = entries.remove(id);
    let at = quota.at.as_deref().ok_or("Codex 额度缺少读取时间")?;
    let (baseline, _) = apply_read(previous, Some(asb_switch::sha256_hex(id)), quota, at);
    let reset = baseline.last_reset.clone();
    entries.insert(id.to_string(), baseline);
    crate::config_store::write_json_atomic(
        &path,
        &serde_json::to_string_pretty(&entries).map_err(|_| "Codex 账号额度历史无法序列化")?,
    )?;
    Ok(reset)
}
