use super::plan::{build_plan_for_profile, preview_projection};
use super::profile_save::invalidate_provider_readings;
use crate::commands::error::CommandError;
use asb_core::contracts::{AppKind, ConfigWriteRecord, WriteOperation};
use asb_switch::io::FsIo;
use asb_switch::{execute, execute_codex};
use tauri::{AppHandle, Manager};

/// Finishes the only kind of interrupted provider save that can leave a
/// durable marker: an already-written active profile awaiting its client
/// projection. The marker contains no secret; the current provider file is
/// revalidated and rendered again before any live client file is touched.
pub(crate) fn recover_pending_profile_save(app: &AppHandle) -> Result<(), String> {
    let state = crate::local_state::LocalState::from_app(app)?;
    let gateway = app
        .state::<crate::gateway::GatewayController>()
        .inner()
        .clone();
    let Some(pending) = state
        .configuration()
        .pending_profile_save()
        .map_err(|error| error.to_string())?
    else {
        return Ok(());
    };
    let record = state
        .configuration()
        .find_provider_record(&pending.profile_id)?;
    if record.profile.app != pending.app {
        return Err("供应商保存恢复记录与当前档案不匹配".to_string());
    }
    // A process can stop after writing the marker but before replacing the
    // provider file. In that case the old revision is still authoritative and
    // recovery must discard the marker without touching client configuration.
    if record.file_hash == pending.previous_file_hash {
        state.configuration().clear_profile_save()?;
        return Ok(());
    }
    let profile = record.profile;
    let projection =
        build_plan_for_profile(&state, &gateway, profile).map_err(|error| error.message)?;
    let preview = preview_projection(&state, &projection).map_err(|error| error.message)?;
    let plan = &projection.plan;
    let target = state
        .target(plan.app())
        .map_err(|error| error.to_string())?;
    let backup_dir = state.backup_dir();
    let write = ConfigWriteRecord {
        app: plan.app(),
        profile_id: Some(plan.profile.id.clone()),
        profile_name: Some(plan.profile.name.clone()),
        content_hash: String::new(),
        backup_id: String::new(),
        at: String::new(),
        operation: WriteOperation::Projection,
    };
    match plan.app() {
        AppKind::Codex => {
            let auth_target = crate::local_state::LocalState::codex_auth_path()?;
            let commit_projection = projection.clone();
            execute_codex(
                &FsIo,
                &asb_switch::CodexSwitchRequest {
                    target: &target,
                    auth_target: &auth_target,
                    plan,
                    backup_dir: &backup_dir,
                    expected_hash: &preview.content_hash,
                    expected_rendered_hash: &preview.rendered_hash,
                },
                |outcome| {
                    gateway.commit(&commit_projection, || {
                        state
                            .configuration()
                            .record_config_write(ConfigWriteRecord {
                                content_hash: outcome.final_hash.clone(),
                                backup_id: outcome.backup.id.clone(),
                                at: outcome.backup.created_at.clone(),
                                ..write
                            })
                    })
                },
            )
        }
        AppKind::Claude => {
            let commit_projection = projection.clone();
            execute(
                &FsIo,
                &asb_switch::SwitchRequest {
                    target: &target,
                    plan,
                    backup_dir: &backup_dir,
                    expected_hash: &preview.content_hash,
                    expected_rendered_hash: &preview.rendered_hash,
                },
                |outcome| {
                    gateway.commit(&commit_projection, || {
                        state
                            .configuration()
                            .record_config_write(ConfigWriteRecord {
                                content_hash: outcome.final_hash.clone(),
                                backup_id: outcome.backup.id.clone(),
                                at: outcome.backup.created_at.clone(),
                                ..write
                            })
                    })
                },
            )
        }
    }
    .map_err(|error| error.to_string())?;
    state.configuration().clear_profile_save()?;
    invalidate_provider_readings(app, &pending.profile_id);
    Ok(())
}

/// Every later configuration write first completes a previously confirmed
/// profile update. The journal is the sole recovery owner for a crash after
/// the provider file changed but before its client projection did.
pub(crate) fn ensure_profile_save_recovered(app: &AppHandle) -> Result<(), CommandError> {
    recover_pending_profile_save(app)
        .map_err(|error| CommandError::new("profile-save-recovery-required", error))
}
