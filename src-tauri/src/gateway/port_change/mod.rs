//! Recoverable gateway endpoint changes.
//!
//! A port change owns exactly one contract: the loopback address. It never
//! replays a provider projection, so model and common settings are outside
//! this transaction. The prepared socket closes the bind race; the durable
//! journal resolves an interrupted multi-client write to one side.

mod commit;
mod prepare;
mod recovery;

#[cfg(test)]
mod tests;

pub(crate) use commit::commit;
pub(crate) use prepare::prepare;
pub(crate) use recovery::{discard_blocked, recover_pending};

use super::*;
use crate::local_state::LocalState;
use asb_core::contracts::{ConfigWriteRecord, WriteOperation};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const JOURNAL_VERSION: u8 = 1;
const PREPARATION_TTL: Duration = Duration::from_secs(10 * 60);
const MAX_PREPARATIONS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayPortChangeClient {
    pub(crate) app: AppKind,
    pub(crate) profile_id: String,
    pub(crate) profile_name: String,
    pub(crate) current_base_url: String,
    pub(crate) new_base_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayPortChangePlan {
    pub(crate) preparation_id: String,
    pub(crate) from_port: u16,
    pub(crate) to_port: u16,
    pub(crate) clients: Vec<GatewayPortChangeClient>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GatewayPortChangeResult {
    pub(crate) from_port: u16,
    pub(crate) to_port: u16,
    pub(crate) clients: Vec<GatewayPortChangeClient>,
    pub(crate) warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BlockedPortChange {
    pub(crate) from_port: u16,
    pub(crate) to_port: u16,
    pub(crate) apps: Vec<AppKind>,
    pub(crate) reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FileSnapshot {
    pub(super) existed: bool,
    pub(super) hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OwnedRouteSnapshot {
    pub(super) profile_id: String,
    pub(super) fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ClientSnapshot {
    pub(super) app: AppKind,
    pub(super) config: FileSnapshot,
    pub(super) codex_auth: Option<FileSnapshot>,
    pub(super) route: Option<OwnedRouteSnapshot>,
}

pub(super) struct ObservedClient {
    pub(super) snapshot: ClientSnapshot,
    pub(super) configuration: Option<String>,
    pub(super) client: Option<GatewayPortChangeClient>,
}

pub(super) struct PreparedPortChange {
    pub(super) created_at: Instant,
    pub(super) plan: GatewayPortChangePlan,
    pub(super) listener: BoundListener,
    pub(super) snapshots: Vec<ClientSnapshot>,
}

#[derive(Clone, Default)]
pub(crate) struct PortChangePreparations {
    inner: Arc<PreparationInner>,
}

#[derive(Default)]
struct PreparationInner {
    entries: Mutex<HashMap<String, PreparedPortChange>>,
    commit_lock: Mutex<()>,
    changed: Condvar,
    reaper_running: Mutex<bool>,
}

impl PortChangePreparations {
    pub(super) fn issue(
        &self,
        mut prepared: PreparedPortChange,
    ) -> Result<GatewayPortChangePlan, String> {
        self.start_reaper()?;
        let id = Uuid::new_v4().to_string();
        prepared.plan.preparation_id = id.clone();
        let mut entries = self
            .inner
            .entries
            .lock()
            .map_err(|_| "端口修改准备状态不可用".to_string())?;
        prune_expired(&mut entries, Instant::now());
        if entries.len() >= MAX_PREPARATIONS {
            if let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.created_at)
                .map(|(id, _)| id.clone())
            {
                entries.remove(&oldest);
            }
        }
        let plan = prepared.plan.clone();
        entries.insert(id, prepared);
        self.inner.changed.notify_one();
        Ok(plan)
    }

    pub(super) fn take(&self, id: &str) -> Result<PreparedPortChange, String> {
        let mut entries = self
            .inner
            .entries
            .lock()
            .map_err(|_| "端口修改准备状态不可用".to_string())?;
        prune_expired(&mut entries, Instant::now());
        entries
            .remove(id)
            .ok_or_else(|| "端口修改预览已失效，请重新发起修改".to_string())
    }

    pub(crate) fn cancel(&self, id: &str) -> Result<bool, String> {
        let mut entries = self
            .inner
            .entries
            .lock()
            .map_err(|_| "端口修改准备状态不可用".to_string())?;
        let removed = entries.remove(id);
        self.inner.changed.notify_one();
        if let Some(prepared) = removed {
            // A prepared listener has no serve worker yet, so dropping its
            // final owner releases the socket. Signal it first as well: this
            // keeps cancellation correct if preparation ever gains a probe
            // worker before ownership returns here.
            prepared.listener.stop();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub(crate) fn lock_commit(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.inner
            .commit_lock
            .lock()
            .map_err(|_| "端口修改事务锁不可用".to_string())
    }

    fn start_reaper(&self) -> Result<(), String> {
        let mut running = self
            .inner
            .reaper_running
            .lock()
            .map_err(|_| "端口预览回收状态不可用".to_string())?;
        if *running {
            return Ok(());
        }
        let inner = Arc::downgrade(&self.inner);
        thread::Builder::new()
            .name("asb-gateway-port-preview-reaper".to_string())
            .spawn(move || reap_preparations(inner))
            .map_err(|_| "无法启动端口预览回收任务".to_string())?;
        *running = true;
        Ok(())
    }
}

fn reap_preparations(inner: std::sync::Weak<PreparationInner>) {
    loop {
        let Some(inner) = inner.upgrade() else {
            return;
        };
        let Ok(mut entries) = inner.entries.lock() else {
            return;
        };
        let now = Instant::now();
        prune_expired(&mut entries, now);
        let wait = entries
            .values()
            .map(|entry| {
                PREPARATION_TTL.saturating_sub(now.saturating_duration_since(entry.created_at))
            })
            .min()
            .unwrap_or(Duration::from_secs(60));
        let _ = inner.changed.wait_timeout(entries, wait);
    }
}

fn prune_expired(entries: &mut HashMap<String, PreparedPortChange>, now: Instant) {
    entries.retain(|_, entry| now.saturating_duration_since(entry.created_at) < PREPARATION_TTL);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum JournalStage {
    Prepared,
    Committed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct JournalClient {
    pub(super) app: AppKind,
    pub(super) before_hash: String,
    pub(super) after_hash: String,
    pub(super) backup_file: String,
    pub(super) history: Option<ConfigWriteRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct PortChangeJournal {
    pub(super) version: u8,
    pub(super) id: String,
    pub(super) from_port: u16,
    pub(super) to_port: u16,
    pub(super) stage: JournalStage,
    pub(super) clients: Vec<JournalClient>,
}

pub(super) fn journal_path(local: &LocalState) -> PathBuf {
    local
        .gateway_state_path()
        .with_file_name("gateway-port-journal.json")
}

pub(super) fn backup_root(local: &LocalState) -> PathBuf {
    local.backup_dir().join("gateway-port")
}

pub(super) fn backup_dir(local: &LocalState, id: &str) -> Result<PathBuf, String> {
    validate_journal_id(id)?;
    Ok(backup_root(local).join(id))
}

fn validate_journal_id(id: &str) -> Result<(), String> {
    let parsed = Uuid::parse_str(id).map_err(|_| "端口修改恢复记录标识无效".to_string())?;
    if parsed.to_string() != id {
        return Err("端口修改恢复记录标识无效".to_string());
    }
    Ok(())
}

pub(super) fn write_journal(local: &LocalState, journal: &PortChangeJournal) -> Result<(), String> {
    validate_journal(journal)?;
    let content =
        serde_json::to_vec_pretty(journal).map_err(|_| "无法序列化端口修改恢复记录".to_string())?;
    write_journal_atomic(&journal_path(local), &content)
}

pub(super) fn read_journal(local: &LocalState) -> Result<Option<PortChangeJournal>, String> {
    let path = journal_path(local);
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("端口修改恢复记录不可读".to_string()),
    };
    let journal: PortChangeJournal =
        serde_json::from_str(&text).map_err(|_| "端口修改恢复记录格式无效".to_string())?;
    validate_journal(&journal)?;
    Ok(Some(journal))
}

fn validate_journal(journal: &PortChangeJournal) -> Result<(), String> {
    if journal.version != JOURNAL_VERSION {
        return Err(format!(
            "端口修改恢复记录版本不受支持（{}）",
            journal.version
        ));
    }
    validate_journal_id(&journal.id)?;
    validate_custom_port(journal.from_port)?;
    validate_custom_port(journal.to_port)?;
    if journal.from_port == journal.to_port {
        return Err("端口修改恢复记录端口无效".to_string());
    }
    let mut seen_apps = Vec::with_capacity(journal.clients.len());
    for client in &journal.clients {
        if seen_apps.contains(&client.app) {
            return Err("端口修改恢复记录重复包含客户端".to_string());
        }
        seen_apps.push(client.app);
        if client.before_hash.len() != 64
            || client.after_hash.len() != 64
            || !client
                .before_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || !client
                .after_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || client.backup_file != backup_file_for(client.app)
        {
            return Err("端口修改恢复记录客户端内容无效".to_string());
        }
        if let Some(record) = &client.history {
            crate::config_store::history::validate_write_record(client.app, record)
                .map_err(|_| "端口修改恢复记录写入历史无效".to_string())?;
            if record.operation != WriteOperation::GatewayPortChange
                || record.content_hash != client.after_hash
            {
                return Err("端口修改恢复记录写入历史不匹配".to_string());
            }
        }
    }
    if journal.stage == JournalStage::Committed
        && journal
            .clients
            .iter()
            .any(|client| client.history.is_none())
    {
        return Err("已提交的端口修改恢复记录缺少客户端写入历史".to_string());
    }
    Ok(())
}

pub(super) fn backup_file_for(app: AppKind) -> &'static str {
    match app {
        AppKind::Codex => "codex-config.toml",
        AppKind::Claude => "claude-settings.json",
    }
}

fn write_journal_atomic(path: &Path, content: &[u8]) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "恢复记录路径无效".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "无法创建恢复记录目录".to_string())?;
    let temporary = parent.join(format!(".asb-{}.tmp", Uuid::new_v4().simple()));
    fs::write(&temporary, content).map_err(|_| "无法写入恢复记录".to_string())?;
    if fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
        return Err("无法原子保存恢复记录".to_string());
    }
    Ok(())
}

pub(super) fn hash_bytes(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub(super) fn blocked(journal: &PortChangeJournal, reason: impl Into<String>) -> BlockedPortChange {
    BlockedPortChange {
        from_port: journal.from_port,
        to_port: journal.to_port,
        apps: journal.clients.iter().map(|client| client.app).collect(),
        reason: reason.into(),
    }
}
