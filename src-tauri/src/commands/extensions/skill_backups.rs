use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::ExtensionPayload;
use serde::Serialize;
use tauri::AppHandle;

use super::support::{extension_mutation, extension_store, store_error, ExtensionMutationDto};
use crate::commands::error::{blocking, require_write_confirmation, state, CommandError};
use crate::extensions::store::SkillBackup;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillBackupDto {
    id: String,
    definition_id: String,
    name: String,
    description: Option<String>,
    host_scoped: Option<AppKind>,
    content_digest: String,
    deleted_at: String,
}

fn backup_view(backup: SkillBackup) -> SkillBackupDto {
    let ExtensionPayload::Skill(skill) = backup.definition.payload else {
        unreachable!()
    };
    SkillBackupDto {
        id: backup.id,
        definition_id: backup.definition.id,
        name: backup.definition.name,
        description: skill.manifest.description,
        host_scoped: skill.host_scoped,
        content_digest: skill.content_digest,
        deleted_at: backup.deleted_at,
    }
}

#[tauri::command]
pub async fn list_skill_backups(app: AppHandle) -> Result<Vec<SkillBackupDto>, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        extension_store(&state)
            .list_skill_backups()
            .map(|backups| backups.into_iter().map(backup_view).collect())
            .map_err(store_error)
    })
    .await
}

#[tauri::command]
pub async fn restore_skill_backup(
    app: AppHandle,
    backup_id: String,
    confirm_write: bool,
) -> Result<ExtensionMutationDto, CommandError> {
    require_write_confirmation(confirm_write, "恢复 Skill 备份")?;
    blocking(move || {
        let state = state(&app)?;
        let definition = extension_store(&state)
            .restore_skill_backup(&backup_id)
            .map_err(store_error)?;
        Ok(extension_mutation(&definition))
    })
    .await
}

#[tauri::command]
pub async fn delete_skill_backup(
    app: AppHandle,
    backup_id: String,
    confirm_write: bool,
) -> Result<(), CommandError> {
    require_write_confirmation(confirm_write, "删除 Skill 备份")?;
    blocking(move || {
        let state = state(&app)?;
        extension_store(&state)
            .delete_skill_backup(&backup_id)
            .map_err(store_error)
    })
    .await
}
