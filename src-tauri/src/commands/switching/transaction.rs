//! Application half of a configuration transaction; the executor owns file recovery.
mod catalog;
mod journal;

use super::codex_backfill::{AppliedCodexBackfill, PreparedCodexBackfill};
use crate::commands::error::CommandError;
use crate::{
    gateway::{GatewayActivationSnapshot, GatewayController},
    local_state::LocalState,
};
use asb_core::{AppKind, ConfigWriteRecord, WriteOperation};
use asb_switch::{FsIo, PendingConfigWrite, SwitchIo};
#[cfg(test)]
pub(super) use catalog::apply_catalog_artifact;
use catalog::{restore_catalog, validate_catalog_target, verify_catalog_after};
pub(super) use catalog::{stage_catalog, CatalogArtifact};
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
    #[serde(default)]
    codex_backfill: Option<CodexBackfillIntent>,
    #[serde(default)]
    auth: Option<AuthIntent>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct AuthIntent {
    pub(super) before_hash: String,
    pub(super) before_existed: bool,
    pub(super) after_hash: String,
    pub(super) after_existed: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CodexBackfillIntent {
    profile_id: String,
    before_hash: String,
    after_hash: String,
    before: asb_core::contracts::CodexProviderFile,
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
    begin_with_codex_backfill(
        state,
        gateway,
        app,
        profile_id,
        after_hash,
        after_existed,
        catalog,
        None,
    )
}

pub(super) fn begin_with_codex_backfill(
    state: &LocalState,
    gateway: &GatewayController,
    app: AppKind,
    profile_id: Option<&str>,
    after_hash: &str,
    after_existed: bool,
    catalog: Option<CatalogArtifact>,
    codex_backfill: Option<&PreparedCodexBackfill>,
) -> Result<(), CommandError> {
    begin_with_codex_backfill_and_auth(
        state,
        gateway,
        app,
        profile_id,
        after_hash,
        after_existed,
        catalog,
        codex_backfill,
        None,
    )
}

pub(super) fn begin_with_codex_backfill_and_auth(
    state: &LocalState,
    gateway: &GatewayController,
    app: AppKind,
    profile_id: Option<&str>,
    after_hash: &str,
    after_existed: bool,
    catalog: Option<CatalogArtifact>,
    codex_backfill: Option<&PreparedCodexBackfill>,
    auth: Option<AuthIntent>,
) -> Result<(), CommandError> {
    if load(state).map_err(error)?.is_some() {
        return Err(error("存在未完成配置事务，请先恢复"));
    }
    let target = state.target(app).map_err(error)?;
    let (before, before_existed) = read_target(&target, app).map_err(error)?;
    if let Some(auth) = &auth {
        if app != AppKind::Codex
            || !auth_snapshot_matches(&target, &auth.before_hash, auth.before_existed)
                .map_err(error)?
        {
            return Err(error("Codex 认证文件在预览后已发生变化，请重新查看差异"));
        }
    }
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
        codex_backfill: codex_backfill.map(|backfill| CodexBackfillIntent {
            profile_id: backfill.profile_id.clone(),
            before_hash: backfill.before_hash.clone(),
            after_hash: backfill.after_hash.clone(),
            before: backfill.before.clone(),
        }),
        auth,
    };
    let journal = path(state);
    let text = serde_json::to_string(&intent).expect("intent serializes");
    crate::config_store::write_json_atomic(&journal, &text).map_err(error)?;
    Ok(())
}

fn auth_snapshot_matches(
    config_target: &Path,
    expected_hash: &str,
    expected_existed: bool,
) -> Result<bool, String> {
    let target = config_target.with_file_name("auth.json");
    let (content, existed) = match FsIo.read_file(&target) {
        Ok(content) => (content, true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (String::new(), false),
        Err(error) => return Err(format!("无法读取 Codex 认证文件：{error}")),
    };
    Ok(existed == expected_existed && asb_switch::sha256_hex(&content) == expected_hash)
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
                    if intent.codex_backfill.is_some() {
                        restore_application_snapshot(state, gateway, &intent, None)
                            .map_err(error)?;
                    } else {
                        restore_catalog(&intent).map_err(error)?;
                    }
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
    let auth_before = auth_matches(&intent, false)?;
    let auth_after = auth_matches(&intent, true)?;
    let unchanged_application = state
        .configuration()
        .latest_config_write(intent.app)
        .map_err(|e| e.to_string())?
        == intent.previous_write;
    if pending.is_none()
        && unchanged_application
        && hash == intent.before_hash
        && existed == intent.before_existed
        && auth_before
    {
        if !resume_active_save(state, gateway, &intent, None, &text)? {
            rollback_application(state, gateway, &intent, None)?;
        }
    } else if hash == intent.after_hash && existed == intent.after_existed && auth_after {
        complete(state, gateway, &intent, pending.as_ref(), &text)?;
    } else if hash == intent.before_hash && existed == intent.before_existed && auth_before {
        if !resume_active_save(state, gateway, &intent, pending.as_ref(), &text)? {
            rollback_application(state, gateway, &intent, pending.as_ref())?;
        }
    } else if pending.is_some()
        && snapshot_is_expected(
            hash == intent.before_hash,
            hash == intent.after_hash,
            auth_before,
            auth_after,
        )
    {
        rollback_pending_transaction(state, gateway, &intent, pending.as_ref().unwrap())?;
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

fn rollback_pending_transaction(
    state: &LocalState,
    gateway: &GatewayController,
    intent: &SwitchIntent,
    pending: &PendingConfigWrite,
) -> Result<(), String> {
    asb_switch::rollback_pending_config(&FsIo, &state.backup_dir(), pending, || {
        restore_application_snapshot(state, gateway, intent, Some(pending))
    })
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn snapshot_is_expected(
    config_before: bool,
    config_after: bool,
    auth_before: bool,
    auth_after: bool,
) -> bool {
    (config_before || config_after)
        && (auth_before || auth_after)
        && !(config_before && auth_before)
        && !(config_after && auth_after)
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
    validate_codex_backfill_after(state, intent)?;
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
    restore_codex_backfill(state, intent)?;
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

fn backfill_from_intent(intent: &SwitchIntent) -> Option<AppliedCodexBackfill> {
    intent
        .codex_backfill
        .as_ref()
        .map(|backfill| AppliedCodexBackfill {
            profile_id: backfill.profile_id.clone(),
            before: backfill.before.clone(),
            before_hash: backfill.before_hash.clone(),
            after_hash: backfill.after_hash.clone(),
        })
}

fn validate_codex_backfill_after(state: &LocalState, intent: &SwitchIntent) -> Result<(), String> {
    let Some(backfill) = backfill_from_intent(intent) else {
        return Ok(());
    };
    let current = state
        .configuration()
        .list_codex_providers()
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|record| record.profile.id == backfill.profile_id)
        .ok_or_else(|| "已确认的 Codex 回填供应商不存在".to_string())?;
    if current.file_hash != backfill.after_hash {
        return Err("已确认的 Codex 回填供应商版本已变化，不能重解释未完成事务".into());
    }
    Ok(())
}

fn restore_codex_backfill(state: &LocalState, intent: &SwitchIntent) -> Result<(), String> {
    let Some(backfill) = backfill_from_intent(intent) else {
        return Ok(());
    };
    super::codex_backfill::restore(state, &backfill)
}

pub(super) fn profile_revision(
    state: &LocalState,
    app: AppKind,
    id: &str,
) -> Result<String, String> {
    profile_name_and_revision(state, app, id).map(|(_, revision)| revision)
}

fn profile_name_and_revision(
    state: &LocalState,
    app: AppKind,
    id: &str,
) -> Result<(String, String), String> {
    if app == AppKind::Codex {
        if let Some(record) = state
            .configuration()
            .list_codex_providers()
            .map_err(|error| error.to_string())?
            .into_iter()
            .find(|record| record.profile.id == id)
        {
            return Ok((record.profile.name, record.file_hash));
        }
    }
    let record = state
        .configuration()
        .find_provider_record(id)
        .map_err(|error| error.to_string())?;
    if record.profile.app != app {
        return Err("供应商不属于当前配置事务的客户端".into());
    }
    Ok((record.profile.name, record.file_hash))
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
    match (&intent.auth, &pending.auth) {
        (None, None) => {}
        (Some(expected), Some(actual))
            if actual.backup.target_path
                == Path::new(&intent.target)
                    .with_file_name("auth.json")
                    .to_string_lossy()
                && actual.backup.content_hash == expected.before_hash
                && actual.backup.target_existed == expected.before_existed
                && actual.after_hash == expected.after_hash
                && actual.after_existed == expected.after_existed => {}
        _ => return Err("应用认证事务与执行器事务不匹配，保留恢复记录".into()),
    }
    Ok(())
}

fn auth_matches(intent: &SwitchIntent, after: bool) -> Result<bool, String> {
    let Some(auth) = intent.auth.as_ref() else {
        return Ok(true);
    };
    let expected_hash = if after {
        &auth.after_hash
    } else {
        &auth.before_hash
    };
    let expected_existed = if after {
        auth.after_existed
    } else {
        auth.before_existed
    };
    auth_snapshot_matches(Path::new(&intent.target), expected_hash, expected_existed)
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
