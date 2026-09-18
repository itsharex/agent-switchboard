//! A small sidecar transaction couples application policy with the executor's
//! native-file transaction. Recovery only restores proven policy/route snapshots.
use super::preparations::Prepared;
use crate::gateway::codex::policy;
use crate::gateway::{GatewayActivationSnapshot, GatewayController};
use crate::local_state::LocalState;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    version: u8,
    target: String,
    before_policy: Option<String>,
    after_policy: String,
    before_config_hash: String,
    after_config_hash: String,
    before_auth_hash: Option<String>,
    after_auth_hash: Option<String>,
    before_route: GatewayActivationSnapshot,
    after_route: GatewayActivationSnapshot,
}

pub(super) fn begin(
    state: &LocalState,
    gateway: &GatewayController,
    prepared: &Prepared,
) -> Result<(), String> {
    let path = policy::pending_path(state.root());
    if path.exists() {
        return Err("Codex 网关策略存在未完成事务".into());
    }
    let target = state.target(asb_core::AppKind::Codex)?;
    let before_policy = crate::config_store::read_optional(&policy::path(state.root()))
        .map_err(|_| "Codex 策略预映像不可读")?;
    if asb_switch::sha256_hex(before_policy.as_deref().unwrap_or("")) != prepared.policy_revision {
        return Err("Codex 网关策略已被外部修改".into());
    }
    let journal = Journal {
        version: 1,
        target: target.to_string_lossy().into_owned(),
        before_policy,
        after_policy: serde_json::to_string_pretty(&prepared.policy)
            .map_err(|_| "Codex 策略序列化失败")?,
        before_config_hash: prepared.preview.content_hash.clone(),
        after_config_hash: prepared.preview.rendered_hash.clone(),
        before_auth_hash: prepared.preview.auth_hash.clone(),
        after_auth_hash: prepared.preview.auth_rendered_hash.clone(),
        before_route: gateway.snapshot_activation(asb_core::AppKind::Codex)?,
        after_route: prepared.projection.activation_snapshot(),
    };
    let json = serde_json::to_string_pretty(&journal).map_err(|_| "Codex 策略事务序列化失败")?;
    crate::config_store::write_json_atomic(&path, &json)
}

pub(crate) fn recover(
    state: &LocalState,
    gateway: &GatewayController,
    prefer_before: bool,
) -> Result<(), String> {
    let Some(journal) = load(state.root())? else {
        return Ok(());
    };
    let target = state.target(asb_core::AppKind::Codex)?;
    if target != Path::new(&journal.target) {
        return Err("Codex 配置目录已改变；保留原网关策略事务等待恢复".into());
    }
    let config = hash_file(&target)?;
    let auth = if journal.before_auth_hash.is_some() || journal.after_auth_hash.is_some() {
        Some(hash_file(&target.with_file_name("auth.json"))?)
    } else {
        None
    };
    let current = crate::config_store::read_optional(&policy::path(state.root()))
        .map_err(|_| "Codex 策略不可读")?;
    let before = recovery_side(
        &journal,
        &config,
        auth.as_deref(),
        current.as_deref(),
        prefer_before,
    )?;
    restore_policy(
        state.root(),
        if before {
            journal.before_policy.as_deref()
        } else {
            Some(&journal.after_policy)
        },
    )?;
    gateway.restore_activation_snapshot(
        state,
        if before {
            &journal.before_route
        } else {
            &journal.after_route
        },
    )?;
    clear(state.root())
}

fn recovery_side(
    journal: &Journal,
    config_hash: &str,
    auth_hash: Option<&str>,
    current_policy: Option<&str>,
    prefer_before: bool,
) -> Result<bool, String> {
    let policy_before = current_policy == journal.before_policy.as_deref();
    if !policy_before && current_policy != Some(journal.after_policy.as_str()) {
        return Err("Codex 网关策略已被外部修改；未覆盖外部数据".into());
    }
    let before = config_hash == journal.before_config_hash
        && journal
            .before_auth_hash
            .as_deref()
            .is_none_or(|hash| Some(hash) == auth_hash);
    let after = config_hash == journal.after_config_hash
        && journal
            .after_auth_hash
            .as_deref()
            .is_none_or(|hash| Some(hash) == auth_hash);
    match (before, after) {
        (true, true) => Ok(prefer_before || policy_before),
        (true, false) => Ok(true),
        (false, true) => Ok(false),
        _ => Err("Codex 配置或认证已被外部修改；保留网关事务，不覆盖用户文件".into()),
    }
}

fn load(root: &Path) -> Result<Option<Journal>, String> {
    let raw = crate::config_store::read_optional(&policy::pending_path(root))
        .map_err(|_| "Codex 网关事务不可读")?;
    raw.map(|raw| {
        let journal: Journal = serde_json::from_str(&raw).map_err(|_| "Codex 网关事务格式无效")?;
        if journal.version != 1
            || journal.before_route.app != asb_core::AppKind::Codex
            || journal.after_route.app != asb_core::AppKind::Codex
        {
            return Err("Codex 网关事务版本或客户端无效".into());
        }
        for raw in [
            journal.before_policy.as_deref(),
            Some(&journal.after_policy),
        ]
        .into_iter()
        .flatten()
        {
            serde_json::from_str::<policy::CodexGatewayPolicy>(raw)
                .map_err(|_| "Codex 网关事务策略无效")?
                .validate()?;
        }
        Ok(journal)
    })
    .transpose()
}
fn hash_file(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(asb_switch::sha256_hex(&raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(asb_switch::sha256_hex(""))
        }
        Err(_) => Err("Codex 事务关联文件不可读".into()),
    }
}
fn restore_policy(root: &Path, raw: Option<&str>) -> Result<(), String> {
    match raw {
        Some(raw) => crate::config_store::write_json_atomic(&policy::path(root), raw),
        None => remove(&policy::path(root)),
    }
}
fn remove(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("无法清理 Codex 网关事务文件：{error}")),
    }
}
pub(super) fn clear(root: &Path) -> Result<(), String> {
    remove(&policy::pending_path(root))
}

pub(super) fn abandon(
    state: &LocalState,
    gateway: &GatewayController,
    expected_hash: &str,
) -> Result<(), String> {
    let Some(journal) = load(state.root())? else {
        return Ok(());
    };
    let target = state.target(asb_core::AppKind::Codex)?;
    if target != Path::new(&journal.target) || hash_file(&target)? != expected_hash {
        return Err("Codex 当前文件已变化，请重新查看后再放弃事务".into());
    }
    let current = crate::config_store::read_optional(&policy::path(state.root()))
        .map_err(|_| "Codex 策略不可读")?;
    if current != journal.before_policy && current.as_deref() != Some(&journal.after_policy) {
        return Err("Codex 策略已被外部修改，不能自动放弃事务".into());
    }
    let backup = state.root().join(format!(
        "codex/abandoned-policy-{}.json",
        uuid::Uuid::new_v4()
    ));
    let raw = serde_json::to_string_pretty(&journal).map_err(|_| "Codex 事务备份序列化失败")?;
    crate::config_store::write_json_atomic(&backup, &raw)?;
    restore_policy(state.root(), journal.before_policy.as_deref())?;
    gateway.clear_codex_activation()?;
    clear(state.root())
}

