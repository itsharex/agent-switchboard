use std::fs;
use std::path::PathBuf;

use asb_core::extensions::contracts::{ExtensionDefinition, ExtensionPayload};
use asb_core::extensions::validate::validate_definition;
use serde::{Deserialize, Serialize};

use super::commit::save_lock;
use super::{ExtensionStore, ExtensionStoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillBackup {
    schema_version: u8,
    pub id: String,
    pub deleted_at: String,
    pub definition: ExtensionDefinition,
}

fn validate_backup(backup: &SkillBackup) -> Result<(), ExtensionStoreError> {
    if backup.schema_version != 1
        || !matches!(backup.definition.payload, ExtensionPayload::Skill(_))
    {
        return Err(ExtensionStoreError::Unsupported(
            "Skill 备份格式无效".into(),
        ));
    }
    validate_definition(&backup.definition)
        .map_err(|error| ExtensionStoreError::Unsupported(error.message))
}

impl ExtensionStore {
    fn skill_backup_path(&self, id: &str) -> Result<PathBuf, ExtensionStoreError> {
        let valid = id.strip_prefix("skill-backup-").is_some_and(|suffix| {
            suffix.len() == 32 && suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
        });
        if !valid {
            return Err(ExtensionStoreError::Unsupported(
                "Skill 备份标识无效".into(),
            ));
        }
        Ok(self.path(&["skill-backups", &format!("{id}.json")]))
    }

    fn read_skill_backup(&self, id: &str) -> Result<SkillBackup, ExtensionStoreError> {
        let backup: SkillBackup = self.read_json(&self.skill_backup_path(id)?)?;
        validate_backup(&backup)?;
        if backup.id != id {
            return Err(ExtensionStoreError::Unsupported(
                "Skill 备份标识与文件不一致".into(),
            ));
        }
        Ok(backup)
    }

    pub fn list_skill_backups(&self) -> Result<Vec<SkillBackup>, ExtensionStoreError> {
        let entries = match fs::read_dir(self.path(&["skill-backups"])) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(ExtensionStoreError::Unreadable(error.to_string())),
        };
        let mut backups = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|error| ExtensionStoreError::Unreadable(error.to_string()))?;
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                let id = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .ok_or_else(|| {
                        ExtensionStoreError::Unsupported("Skill 备份文件名无效".into())
                    })?;
                backups.push(self.read_skill_backup(id)?);
            }
        }
        backups.sort_by(|left, right| {
            right
                .deleted_at
                .cmp(&left.deleted_at)
                .then(left.id.cmp(&right.id))
        });
        Ok(backups)
    }

    fn back_up_skill(&self, definition: &ExtensionDefinition) -> Result<(), ExtensionStoreError> {
        let ExtensionPayload::Skill(skill) = &definition.payload else {
            return Ok(());
        };
        // Immutable bytes already belong to this library. Verify them before
        // deleting the definition and keep its identity for historical restores.
        self.load_skill_version(&definition.id, &skill.content_digest)?;
        let backup = SkillBackup {
            schema_version: 1,
            id: format!("skill-backup-{}", uuid::Uuid::new_v4().simple()),
            deleted_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            definition: definition.clone(),
        };
        self.write_json(&self.skill_backup_path(&backup.id)?, &backup)
    }

    pub fn delete_definition_if_unbound(&self, id: &str) -> Result<(), ExtensionStoreError> {
        let _guard = save_lock();
        let current = self
            .get_definition(id)?
            .ok_or_else(|| ExtensionStoreError::Conflict("扩展不存在或已被删除".into()))?;
        if self
            .list_bindings()?
            .iter()
            .any(|binding| binding.resource_id == id)
        {
            return Err(ExtensionStoreError::Conflict(
                "该扩展仍有部署绑定；请先从客户端移除".into(),
            ));
        }
        let path = self.path(&["definitions", &format!("{id}.json")]);
        let manifest_before = self.manifest_state_locked()?;
        self.back_up_skill(&current)?;
        fs::remove_file(&path)
            .map_err(|error| ExtensionStoreError::Unreadable(error.to_string()))?;
        if let Err(error) = self.bump_generation_locked() {
            return self.metadata_failure(
                "删除扩展定义",
                error,
                self.restore_file_and_manifest_locked(
                    &path,
                    Some(&current),
                    manifest_before.as_ref(),
                ),
            );
        }
        Ok(())
    }

    pub fn restore_skill_backup(
        &self,
        id: &str,
    ) -> Result<ExtensionDefinition, ExtensionStoreError> {
        let backup = self.read_skill_backup(id)?;
        let ExtensionPayload::Skill(skill) = &backup.definition.payload else {
            unreachable!()
        };
        self.load_skill_version(&backup.definition.id, &skill.content_digest)?;
        if let Some(current) = self.get_definition(&backup.definition.id)? {
            if current == backup.definition {
                return Ok(current);
            }
            return Err(ExtensionStoreError::Conflict(
                "该 Skill 已在扩展库中且内容发生变化；未覆盖现有定义".into(),
            ));
        }
        self.create_definition(&backup.definition)?;
        Ok(backup.definition)
    }

    pub fn delete_skill_backup(&self, id: &str) -> Result<(), ExtensionStoreError> {
        let _guard = save_lock();
        self.read_skill_backup(id)?;
        // History and other backups may still reference these immutable bytes.
        fs::remove_file(self.skill_backup_path(id)?)
            .map_err(|error| ExtensionStoreError::Unreadable(error.to_string()))
    }
}

