//! Application half of a configuration transaction; the executor owns file recovery.
mod catalog;
mod journal;

use crate::commands::error::CommandError;
use crate::{
    gateway::{GatewayActivationSnapshot, GatewayController},
    local_state::LocalState,
};
use asb_core::{AppKind, ConfigWriteRecord, WriteOperation};
use asb_switch::{FsIo, PendingConfigWrite, SwitchIo};
pub(super) use catalog::{apply_catalog_artifact, CatalogArtifact};
use catalog::{restore_catalog, validate_catalog_target, verify_catalog_after};
use journal::{clear, error, load, path, read_target};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct SwitchIntent {
    version: u8,
    app: AppKind,
    target: String,
    profile_id: Option<String>,
    profile_hash: Option<String>,
    before_hash: String,
    before_existed: bool,
    after_hash: String,
    after_existed: bool,
    previous_route: GatewayActivationSnapshot,
    previous_write: Option<ConfigWriteRecord>,
    catalog: Option<CatalogArtifact>,
}

pub(super) fn begin(
    state: &LocalState,
    gateway: &GatewayController,
    app: AppKind,
    profile_id: Option<&str>,
    after_hash: &str,
    after_existed: bool,
    catalog: Option<CatalogArtifact>,
) -> Result<(), CommandError> {
    if load(state).map_err(error)?.is_some() {
        return Err(error("存在未完成配置事务，请先恢复"));
    }
    let target = state.target(app).map_err(error)?;
    let (before, before_existed) = read_target(&target, app).map_err(error)?;
    validate_catalog_target(&target, catalog.as_ref()).map_err(error)?;
    let profile_hash = profile_id
        .map(|id| profile_revision(state, app, id))
        .transpose()
        .map_err(error)?;
    let intent = SwitchIntent {
        version: 1,
        app,
        target: target.to_string_lossy().into_owned(),
        profile_id: profile_id.map(str::to_string),
        profile_hash,
        before_hash: asb_switch::sha256_hex(&before),
        before_existed,
        after_hash: after_hash.into(),
        after_existed,
        previous_route: gateway.snapshot_activation(app).map_err(error)?,
        previous_write: state
            .configuration()
            .latest_config_write(app)
            .map_err(|e| error(e.to_string()))?,
        catalog,
    };
    let journal = path(state);
    let text = serde_json::to_string(&intent).expect("intent serializes");
    crate::config_store::write_json_atomic(&journal, &text).map_err(error)?;
    Ok(())
}

pub(super) fn finish<T>(
    state: &LocalState,
    gateway: &GatewayController,
    result: Result<T, asb_switch::SwitchError>,
) -> Result<T, CommandError> {
    match result {
        Ok(value) => {
            clear(state).map_err(error)?;
            Ok(value)
        }
        Err(failure) => {
            if let Some(intent) = load(state).map_err(error)? {
                if asb_switch::pending_config_write(&FsIo, &state.backup_dir(), intent.app)
                    .map_err(|e| error(e.to_string()))?
                    .is_none()
                {
                    restore_catalog(&intent).map_err(error)?;
                    clear(state).map_err(error)?;
                    return Err(CommandError::from(failure));
                }
            }
            let (current, before) = match load(state).map_err(error)? {
                Some(intent) => {
                    let (text, exists) =
                        read_target(Path::new(&intent.target), intent.app).map_err(error)?;
                    (
                        asb_switch::sha256_hex(&text),
                        (intent.before_hash, exists == intent.before_existed),
                    )
                }
                None => return Err(CommandError::from(failure)),
            };
            if current != before.0 || !before.1 {
                return Err(error(format!("{failure}；配置补偿未完成，保留事务和备份")));
            }
            // An active profile save restores its profile before restoring the route snapshot.
            if state
                .configuration()
                .pending_profile_save()
                .map_err(|e| error(e.to_string()))?
                .is_some()
            {
                return Err(CommandError::from(failure));
            }
            recover(state, gateway).map_err(error)?;
            Err(CommandError::from(failure))
        }
    }
}

pub(super) fn recover(state: &LocalState, gateway: &GatewayController) -> Result<(), String> {
    let Some(intent) = load(state)? else {
        return reject_orphan(state);
    };
    let target = state.target(intent.app)?;
    if intent.version != 1
        || Path::new(&intent.target) != target
        || intent.previous_route.app != intent.app
    {
        return Err("配置事务目标或版本不匹配，保留恢复记录".into());
    }
    validate_catalog_target(&target, intent.catalog.as_ref())?;
    let pending = asb_switch::pending_config_write(&FsIo, &state.backup_dir(), intent.app)
        .map_err(|e| e.to_string())?;
    if let Some(pending) = &pending {
        validate_pending(&intent, pending)?;
    }
    let (text, existed) = read_target(&target, intent.app)?;
    let hash = asb_switch::sha256_hex(&text);
    let unchanged_application = state
        .configuration()
        .latest_config_write(intent.app)
        .map_err(|e| e.to_string())?
        == intent.previous_write;
    if pending.is_none()
        && unchanged_application
        && hash == intent.before_hash
        && existed == intent.before_existed
    {
        if !resume_active_save(state, gateway, &intent, None, &text)? {
            rollback_application(state, gateway, &intent, None)?;
        }
    } else if hash == intent.after_hash && existed == intent.after_existed {
        complete(state, gateway, &intent, pending.as_ref(), &text)?;
    } else if hash == intent.before_hash && existed == intent.before_existed {
        if !resume_active_save(state, gateway, &intent, pending.as_ref(), &text)? {
            rollback_application(state, gateway, &intent, pending.as_ref())?;
        }
    } else {
        let backup = pending
            .as_ref()
            .map(|p| p.backup.id.as_str())
            .unwrap_or("尚未创建备份");
        return Err(format!(
            "配置与事务前后快照均不符；保留恢复记录，请在备份列表显式恢复事务备份 {backup}"
        ));
    }
    clear(state)
}

fn resume_active_save(
    state: &LocalState,
    gateway: &GatewayController,
    intent: &SwitchIntent,
    pending: Option<&PendingConfigWrite>,
    _current: &str,
) -> Result<bool, String> {
    let Some(save) = state
        .configuration()
        .pending_profile_save()
        .map_err(|e| e.to_string())?
    else {
        return Ok(false);
    };
    if Some(&save.profile_id) != intent.profile_id.as_ref() {
        return Err("供应商保存与配置事务不匹配".into());
    }
    let revision = profile_revision(state, save.app, &save.profile_id)?;
    if revision == save.previous_file_hash {
        return Ok(false);
    }
    if Some(&revision) != intent.profile_hash.as_ref() {
        return Err("待恢复供应商版本已发生额外变化".into());
    }
    let projected =
        super::plan::build_plan(state, gateway, &save.profile_id).map_err(|e| e.message)?;
    let preview = super::plan::preview_projection(state, &projected).map_err(|e| e.message)?;
    if preview.rendered_hash != intent.after_hash {
        return Err("保存后的目标配置已变化，不能重新解释已确认事务".into());
    }
    finish_pending(
        state,
        pending,
        &intent.before_hash,
        intent.before_existed,
        || Ok(()),
    )?;
    Ok(true)
}

fn complete(
    state: &LocalState,
    gateway: &GatewayController,
    intent: &SwitchIntent,
    pending: Option<&PendingConfigWrite>,
    text: &str,
) -> Result<(), String> {
    verify_catalog_after(intent)?;
    let profile = intent
        .profile_id
        .as_deref()
        .map(|id| {
            let (name, hash) = profile_name_and_revision(state, intent.app, id)?;
            if Some(&hash) != intent.profile_hash.as_ref() {
                return Err(String::from(
                    "已确认的供应商版本已变化，不能重解释未完成事务",
                ));
            }
            Ok((id.to_string(), name))
        })
        .transpose()?;
    gateway.validate_restored(state, intent.app, text)?;
    let commit = || {
        gateway.reconcile_restored(state, intent.app, || {
            let last = state
                .configuration()
                .latest_config_write(intent.app)
                .map_err(|e| e.to_string())?;
            if let Some(pending) = pending {
                let desired = ConfigWriteRecord {
                    app: intent.app,
                    profile_id: profile.as_ref().map(|p| p.0.clone()),
                    profile_name: profile.as_ref().map(|p| p.1.clone()),
                    content_hash: intent.after_hash.clone(),
                    backup_id: pending.backup.id.clone(),
                    at: pending.backup.created_at.clone(),
                    operation: if profile.is_some() {
                        WriteOperation::Projection
                    } else {
                        WriteOperation::Restore
                    },
                };
                if last.as_ref().is_some_and(|r| {
                    r.backup_id == desired.backup_id
                        && r.content_hash == desired.content_hash
                        && r.operation == desired.operation
                }) {
                    return Ok(());
                }
                if last != intent.previous_write {
                    return Err("配置写入历史已变化，拒绝叠加恢复".into());
                }
                state
                    .configuration()
                    .record_config_write(desired)
                    .map_err(|error| error.to_string())
            } else if last.as_ref().is_some_and(|r| {
                r.content_hash == intent.after_hash && r.profile_id == intent.profile_id
            }) {
                Ok(())
            } else {
                Err("执行器事务已消失但应用提交记录缺失，保留恢复记录".into())
            }
        })
    };
    finish_pending(
        state,
        pending,
        &intent.after_hash,
        intent.after_existed,
        commit,
    )
}

fn rollback_application(
    state: &LocalState,
    gateway: &GatewayController,
    intent: &SwitchIntent,
    pending: Option<&PendingConfigWrite>,
) -> Result<(), String> {
    let commit = || restore_application_snapshot(state, gateway, intent, pending);
    finish_pending(
        state,
        pending,
        &intent.before_hash,
        intent.before_existed,
        commit,
    )
}

fn restore_application_snapshot(
    state: &LocalState,
    gateway: &GatewayController,
    intent: &SwitchIntent,
    pending: Option<&PendingConfigWrite>,
) -> Result<(), String> {
    restore_catalog(intent)?;
    gateway.restore_activation_snapshot(state, &intent.previous_route)?;
    let last = state
        .configuration()
        .latest_config_write(intent.app)
        .map_err(|e| e.to_string())?;
    if last == intent.previous_write {
        return Ok(());
    }
    if let (Some(last), Some(pending)) = (last, pending) {
        if last.backup_id == pending.backup.id && last.content_hash == pending.after_hash {
            state
                .configuration()
                .remove_config_write_if_last(&last)
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
    }
    Err("回滚前的应用写入历史已变化，保留事务以人工处理".into())
}

fn profile_revision(state: &LocalState, app: AppKind, id: &str) -> Result<String, String> {
    match app {
        AppKind::Codex => state
            .configuration()
            .list_codex_providers()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|record| record.profile.id == id)
            .map(|record| record.file_hash)
            .ok_or_else(|| "Codex 供应商不存在".to_string()),
        AppKind::Claude => state
            .configuration()
            .find_provider_record(id)
            .map(|record| record.file_hash)
            .map_err(|error| error.to_string()),
    }
}

fn profile_name_and_revision(
    state: &LocalState,
    app: AppKind,
    id: &str,
) -> Result<(String, String), String> {
    match app {
        AppKind::Codex => state
            .configuration()
            .list_codex_providers()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|record| record.profile.id == id)
            .map(|record| (record.profile.name, record.file_hash))
            .ok_or_else(|| "Codex 供应商不存在".to_string()),
        AppKind::Claude => state
            .configuration()
            .find_provider_record(id)
            .map(|record| (record.profile.name, record.file_hash))
            .map_err(|error| error.to_string()),
    }
}

fn finish_pending(
    state: &LocalState,
    pending: Option<&PendingConfigWrite>,
    hash: &str,
    existed: bool,
    commit: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    match pending {
        Some(pending) => asb_switch::finish_config_recovery(
            &FsIo,
            &state.backup_dir(),
            pending,
            hash,
            existed,
            commit,
        )
        .map_err(|e| e.to_string()),
        None => commit(),
    }
}

fn validate_pending(intent: &SwitchIntent, pending: &PendingConfigWrite) -> Result<(), String> {
    if pending.backup.target_path != intent.target
        || pending.backup.content_hash != intent.before_hash
        || pending.backup.target_existed != intent.before_existed
        || pending.after_hash != intent.after_hash
        || pending.after_existed != intent.after_existed
        || pending.profile_id != intent.profile_id
    {
        return Err("应用事务与执行器事务不匹配，保留恢复记录".into());
    }
    Ok(())
}

pub(super) fn restore_pending_backup(
    state: &LocalState,
    gateway: &GatewayController,
    backup_id: &str,
) -> Result<Option<asb_switch::RestoreOutcome>, CommandError> {
    let Some(intent) = load(state).map_err(error)? else {
        return Ok(None);
    };
    let Some(pending) = asb_switch::pending_config_write(&FsIo, &state.backup_dir(), intent.app)
        .map_err(|e| error(e.to_string()))?
    else {
        return Ok(None);
    };
    if pending.backup.id != backup_id {
        return Ok(None);
    }
    validate_pending(&intent, &pending).map_err(error)?;
    if Path::new(&intent.target) != state.target(intent.app).map_err(error)? {
        return Err(error("事务目标不属于当前客户端"));
    }
    if state
        .configuration()
        .pending_profile_save()
        .map_err(|e| error(e.to_string()))?
        .is_some()
    {
        super::profile_rollback::restore(
            state,
            intent
                .profile_id
                .as_deref()
                .ok_or_else(|| error("缺少供应商事务标识"))?,
            intent
                .profile_hash
                .as_deref()
                .ok_or_else(|| error("缺少供应商事务版本"))?,
        )
        .map_err(error)?;
    }
    let outcome = asb_switch::rollback_pending_config(&FsIo, &state.backup_dir(), &pending, || {
        restore_application_snapshot(state, gateway, &intent, Some(&pending))?;
        state.configuration().clear_profile_save()
    })
    .map_err(|e| error(e.to_string()))?;
    clear(state).map_err(error)?;
    super::profile_rollback::clear(state).map_err(error)?;
    Ok(Some(outcome))
}

fn reject_orphan(state: &LocalState) -> Result<(), String> {
    for app in [AppKind::Codex, AppKind::Claude] {
        if let Some(pending) = asb_switch::pending_config_write(&FsIo, &state.backup_dir(), app)
            .map_err(|e| e.to_string())?
        {
            return Err(format!("执行器事务缺少应用意图 {}；请退出应用，保留事务 {}、备份 {} 与当前配置 {}，人工核对后恢复配套意图文件；不要直接删除事务或覆盖配置", path(state).display(), asb_switch::config_journal_path(&state.backup_dir(), app).display(), pending.backup.backup_path, pending.backup.target_path));
        }
    }
    Ok(())
}
