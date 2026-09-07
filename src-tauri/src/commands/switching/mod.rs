//! Profile-switch, backup, restore, and undo commands. Each write walks the
//! executor transaction and records an audit entry so it can be undone.

mod backups;
mod plan;
mod profile_save;
mod recovery;

pub use profile_save::ProfileSavePreparation;
pub(crate) use profile_save::ProfileSavePreparations;
pub(crate) use recovery::{ensure_profile_save_recovered, recover_pending_profile_save};

use crate::commands::error::{
    blocking, observe, require_write_confirmation, state, store_error, CommandError,
};
use crate::runtime_log::RuntimeLogAction;
use asb_core::adapter;
use asb_core::contracts::{
    AppKind, BackupRecord, ConfigWriteRecord, KeyChange, ProfileSaveKind, ProviderDraft,
    ProviderRecord, WriteOperation,
};
use asb_switch::io::{FsIo, SwitchIo};
use asb_switch::{execute, execute_codex, sha256_hex, RestoreOutcome, SwitchOutcome};
use backups::{find_backup, local_backups, run_restore};
use plan::{build_plan, preview_projection};
use profile_save::{
    commit_prepared_profile_save, invalidate_provider_readings, prepare_profile_save_data,
    PreparedProfileSave,
};
use std::path::PathBuf;
use std::time::Instant;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

/// Validates a draft and binds it to one one-shot server-side preparation. No
/// provider or client file is modified here.
#[tauri::command]
pub async fn prepare_profile_save(
    app: AppHandle,
    profile_id: Option<String>,
    draft: ProviderDraft,
    expected_file_hash: Option<String>,
) -> Result<ProfileSavePreparation, CommandError> {
    let state = state(&app)?;
    let gateway = app
        .state::<crate::gateway::GatewayController>()
        .inner()
        .clone();
    let preparations = app
        .try_state::<ProfileSavePreparations>()
        .ok_or_else(|| {
            CommandError::new("profile-save-unavailable", "供应商保存准备状态尚未初始化")
        })?
        .inner()
        .clone();
    blocking(move || {
        let (kind, preview) = prepare_profile_save_data(
            &state,
            &gateway,
            profile_id.as_deref(),
            &draft,
            expected_file_hash.as_deref(),
        )?;
        let preparation_id = preparations.issue(PreparedProfileSave {
            created_at: Instant::now(),
            profile_id,
            expected_file_hash,
            draft,
            kind,
            preview: preview.clone(),
        })?;
        Ok(ProfileSavePreparation {
            preparation_id,
            kind,
            preview,
        })
    })
    .await
}

/// Commits exactly the draft bound by [`prepare_profile_save`]. A preparation
/// is one-shot, so duplicate confirmation clicks and stale renderer state
/// cannot execute an old plan.
#[tauri::command]
pub async fn commit_profile_save(
    app: AppHandle,
    preparation_id: String,
    confirm_write: bool,
) -> Result<ProviderRecord, CommandError> {
    let preparations = app
        .try_state::<ProfileSavePreparations>()
        .ok_or_else(|| {
            CommandError::new("profile-save-unavailable", "供应商保存准备状态尚未初始化")
        })?
        .inner()
        .clone();
    let prepared = preparations.take(&preparation_id)?;
    let action = if prepared.kind == ProfileSaveKind::Create {
        RuntimeLogAction::ProfileCreated
    } else {
        RuntimeLogAction::ProfileUpdated
    };
    let invalidates_readings = prepared.kind != ProfileSaveKind::NoChange;
    let refresh_app = app.clone();
    let result = observe(action, async move {
        let state = state(&app)?;
        let gateway = app
            .state::<crate::gateway::GatewayController>()
            .inner()
            .clone();
        blocking(move || {
            let _commit_guard = preparations.lock_commit()?;
            ensure_profile_save_recovered(&app)?;
            commit_prepared_profile_save(&state, &gateway, &prepared, confirm_write)
        })
        .await
    })
    .await;
    if invalidates_readings {
        if let Ok(record) = &result {
            invalidate_provider_readings(&refresh_app, &record.profile.id);
        }
    }
    result
}

#[tauri::command]
pub async fn preview_switch(
    app: AppHandle,
    profile_id: String,
) -> Result<asb_switch::FilePreview, CommandError> {
    let state = state(&app)?;
    let gateway = app
        .state::<crate::gateway::GatewayController>()
        .inner()
        .clone();
    blocking(move || {
        let projection = build_plan(&state, &gateway, &profile_id)?;
        preview_projection(&state, &projection)
    })
    .await
}

#[tauri::command]
pub async fn execute_switch(
    app: AppHandle,
    profile_id: String,
    expected_hash: String,
    expected_rendered_hash: String,
    confirm_write: bool,
) -> Result<SwitchOutcome, CommandError> {
    observe(RuntimeLogAction::ConfigurationSwitched, async move {
        require_write_confirmation(confirm_write, "写入配置")?;
        let state = state(&app)?;
        let gateway = app
            .state::<crate::gateway::GatewayController>()
            .inner()
            .clone();
        blocking(move || {
            ensure_profile_save_recovered(&app)?;
            let projection = build_plan(&state, &gateway, &profile_id)?;
            let codex_projection = projection.clone();
            let claude_projection = projection.clone();
            let plan = &projection.plan;
            let target = state
                .target(plan.app())
                .map_err(|error| CommandError::new("config-path-unavailable", error))?;
            let backup_dir = state.backup_dir();
            let record = ConfigWriteRecord {
                app: plan.app(),
                profile_id: Some(plan.profile.id.clone()),
                profile_name: Some(plan.profile.name.clone()),
                content_hash: String::new(),
                backup_id: String::new(),
                at: String::new(),
                operation: WriteOperation::Projection,
            };
            let mut outcome = match plan.app() {
                AppKind::Codex => {
                    let auth_target = crate::local_state::LocalState::codex_auth_path()
                        .map_err(|error| CommandError::new("codex-auth-path-unavailable", error))?;
                    execute_codex(
                        &FsIo,
                        &asb_switch::CodexSwitchRequest {
                            target: &target,
                            auth_target: &auth_target,
                            plan: &plan,
                            backup_dir: &backup_dir,
                            expected_hash: &expected_hash,
                            expected_rendered_hash: &expected_rendered_hash,
                        },
                        move |outcome| {
                            gateway.commit(&codex_projection, || {
                                state
                                    .configuration()
                                    .record_config_write(ConfigWriteRecord {
                                        content_hash: outcome.final_hash.clone(),
                                        backup_id: outcome.backup.id.clone(),
                                        at: outcome.backup.created_at.clone(),
                                        ..record
                                    })
                            })
                        },
                    )
                }
                AppKind::Claude => execute(
                    &FsIo,
                    &asb_switch::SwitchRequest {
                        target: &target,
                        plan: &plan,
                        backup_dir: &backup_dir,
                        expected_hash: &expected_hash,
                        expected_rendered_hash: &expected_rendered_hash,
                    },
                    move |outcome| {
                        gateway.commit(&claude_projection, || {
                            state
                                .configuration()
                                .record_config_write(ConfigWriteRecord {
                                    content_hash: outcome.final_hash.clone(),
                                    backup_id: outcome.backup.id.clone(),
                                    at: outcome.backup.created_at.clone(),
                                    ..record
                                })
                        })
                    },
                ),
            }
            .map_err(CommandError::from)?;
            outcome.preview.target = target.to_string_lossy().to_string();
            crate::tray::refresh(&app);
            Ok(outcome)
        })
        .await
    })
    .await
}

#[tauri::command]
pub async fn list_backups(app: AppHandle) -> Result<Vec<BackupRecord>, CommandError> {
    let state = state(&app)?;
    blocking(move || local_backups(&state)).await
}

#[tauri::command]
pub async fn restore_backup(
    app: AppHandle,
    backup_id: String,
    confirm_write: bool,
) -> Result<RestoreOutcome, CommandError> {
    observe(RuntimeLogAction::BackupRestored, async move {
        require_write_confirmation(confirm_write, "恢复配置")?;
        let state = state(&app)?;
        let gateway = app
            .state::<crate::gateway::GatewayController>()
            .inner()
            .clone();
        blocking(move || {
            ensure_profile_save_recovered(&app)?;
            let record = find_backup(&state, &backup_id)?;
            let outcome = run_restore(&state, &gateway, &record)?;
            crate::tray::refresh(&app);
            Ok(outcome)
        })
        .await
    })
    .await
}

/// Undoes the most recent switch for one client by restoring the backup that
/// switch created.
#[tauri::command]
pub async fn undo_last_switch(
    app: AppHandle,
    target: AppKind,
    confirm_write: bool,
) -> Result<RestoreOutcome, CommandError> {
    observe(RuntimeLogAction::SwitchUndone, async move {
        require_write_confirmation(confirm_write, "撤回切换")?;
        let state = state(&app)?;
        let gateway = app
            .state::<crate::gateway::GatewayController>()
            .inner()
            .clone();
        blocking(move || {
            ensure_profile_save_recovered(&app)?;
            let last = state
                .configuration()
                .latest_config_write(target)
                .map_err(store_error)?
                .ok_or_else(|| {
                    CommandError::new("undo-unavailable", "该客户端没有可撤回的切换记录")
                })?;
            let record = find_backup(&state, &last.backup_id)?;
            let outcome = run_restore(&state, &gateway, &record)?;
            crate::tray::refresh(&app);
            Ok(outcome)
        })
        .await
    })
    .await
}

/// Owned-key difference between the live file and one backup. `before` is
/// the backup value, `after` the current one.
#[tauri::command]
pub async fn backup_diff(
    app: AppHandle,
    backup_id: String,
) -> Result<Vec<KeyChange>, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let record = find_backup(&state, &backup_id)?;
        let target = state
            .target(record.app)
            .map_err(|error| CommandError::new("config-path-unavailable", error))?;
        let current = FsIo
            .read_file(&target)
            .map_err(|_| CommandError::new("read-current", "无法读取当前配置文件，无法生成差异"))?;
        let backup_text = FsIo
            .read_file(PathBuf::from(&record.backup_path).as_path())
            .map_err(|_| CommandError::new("backup-unreadable", "备份文件不可读，无法生成差异"))?;
        if sha256_hex(&backup_text) != record.content_hash {
            return Err(CommandError::new(
                "backup-hash-mismatch",
                "备份内容与记录哈希不符，拒绝生成差异",
            ));
        }
        adapter::owned_diff(record.app, &current, &backup_text)
            .map_err(|error| CommandError::new("diff-failed", error.to_string()))
    })
    .await
}

/// Opens the app-owned backup directory in the system file manager. The
/// directory is created on demand so an empty history still opens cleanly.
#[tauri::command]
pub async fn open_backup_dir(app: AppHandle) -> Result<(), CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let dir = state.backup_dir();
        std::fs::create_dir_all(&dir)
            .map_err(|_| CommandError::new("backup-dir-unavailable", "无法创建备份目录"))?;
        app.opener()
            .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
            .map_err(|_| CommandError::new("backup-dir-open-failed", "无法打开备份文件夹"))
    })
    .await
}
