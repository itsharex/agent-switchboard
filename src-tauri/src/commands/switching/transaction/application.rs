//! Commit and compensation of application state paired with client configuration.
use super::*;

pub(super) fn complete(
    state: &LocalState,
    gateway: &GatewayController,
    intent: &SwitchIntent,
    pending: Option<&PendingConfigWrite>,
    text: &str,
) -> Result<(), String> {
    validate_saved_profile(state, intent)?;
    verify_catalog_after(intent)?;
    validate_codex_backfill_after(state, intent)?;
    let profile = intent.profile_id.as_deref().map(|id| {
        let (name, hash) = profile_name_and_revision(state, intent.app, id)?;
        if Some(&hash) != intent.profile_hash.as_ref() {
            return Err(String::from("已确认的供应商版本已变化，不能重解释未完成事务"));
        }
        Ok((id.to_string(), name))
    }).transpose()?;
    let client_only = intent.operation == WriteOperation::Projection && intent.profile_id.is_none();
    if !client_only {
        gateway.validate_restored(state, intent.app, text)?;
    }
    let commit = || {
        let save = || commit_application(state, intent, pending, profile.as_ref());
        if client_only {
            save()
        } else {
            gateway.reconcile_restored(state, intent.app, save)
        }
    };
    finish_pending(state, pending, &intent.after_hash, intent.after_existed, commit)
}

fn validate_saved_profile(state: &LocalState, intent: &SwitchIntent) -> Result<(), String> {
    let Some(save) = state.configuration().pending_profile_save().map_err(|e| e.to_string())? else {
        return Ok(());
    };
    if save.app != intent.app || Some(&save.projection_profile_id) != intent.profile_id.as_ref() {
        return Err("供应商保存与配置事务不匹配".into());
    }
    let revision = profile_revision(state, save.app, &save.profile_id)?;
    super::super::profile_rollback::validate_saved_revision(state, &save.profile_id, &revision)
}

fn commit_application(
    state: &LocalState,
    intent: &SwitchIntent,
    pending: Option<&PendingConfigWrite>,
    profile: Option<&(String, String)>,
) -> Result<(), String> {
    let config = state.configuration();
    let last = config.latest_config_write(intent.app).map_err(|error| error.to_string())?;
    if let Some(pending) = pending {
        settings::capture_backup(state, intent, &pending.backup)?;
        let desired = ConfigWriteRecord {
            app: intent.app,
            profile_id: profile.map(|p| p.0.clone()),
            profile_name: profile.map(|p| p.1.clone()),
            content_hash: intent.after_hash.clone(),
            backup_id: pending.backup.id.clone(),
            at: pending.backup.created_at.clone(),
            operation: intent.operation,
        };
        if last.as_ref().is_some_and(|r| {
            r.backup_id == desired.backup_id && r.content_hash == desired.content_hash
                && r.operation == desired.operation
        }) {
            return settings::reconcile(state, intent, true);
        }
        if last != intent.previous_write {
            return Err("配置写入历史已变化，拒绝叠加恢复".into());
        }
        settings::reconcile(state, intent, true)?;
        config.record_config_write(desired).map_err(|error| error.to_string())
    } else if last.as_ref().is_some_and(|r| {
        r.content_hash == intent.after_hash && r.profile_id == intent.profile_id
            && r.operation == intent.operation
    }) {
        settings::reconcile(state, intent, true)
    } else {
        Err("执行器事务已消失但应用提交记录缺失，保留恢复记录".into())
    }
}

pub(super) fn rollback_application(
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

pub(super) fn restore_application_snapshot(
    state: &LocalState,
    gateway: &GatewayController,
    intent: &SwitchIntent,
    pending: Option<&PendingConfigWrite>,
) -> Result<(), String> {
    restore_catalog(intent)?;
    restore_codex_backfill(state, intent)?;
    if let Some(pending) = pending { settings::capture_backup(state, intent, &pending.backup)?; }
    settings::reconcile(state, intent, false)?;
    if intent.operation != WriteOperation::Projection || intent.profile_id.is_some() {
        gateway.restore_activation_snapshot(state, &intent.previous_route)?;
    }
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
    super::super::codex_backfill::restore(state, &backfill)
}
