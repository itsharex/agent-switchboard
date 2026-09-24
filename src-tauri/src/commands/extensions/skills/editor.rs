//! Skill update and authoring: upstream update checks, manifest
//! updates, local skill creation and forking, the file editor with
//! immutable versions, and dependency links.

use std::collections::{BTreeMap, BTreeSet};

use asb_core::extensions::contracts::{
    ExtensionKind, ExtensionPayload, SkillDependency, SkillManifest,
};
use asb_core::extensions::skill::ContentEntry;
use asb_core::extensions::validate::validate_definition;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::commands::error::{blocking, state, CommandError};

use super::EDITOR_MAX_TEXT_BYTES;
use crate::commands::extensions::support::*;

// ------------------------------------------------------------- file editor

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEditorFileDto {
    relative_path: String,
    /// None for files the editor cannot show (binary or oversized); their
    /// bytes are carried over unchanged on save.
    text: Option<String>,
    size: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillEditorViewDto {
    id: String,
    revision: u64,
    content_digest: String,
    manifest: SkillManifest,
    /// Whether update_skill_files accepts edits for this definition; skills
    /// with a source or host scoping must be forked first.
    editable: bool,
    files: Vec<SkillEditorFileDto>,
}

/// Loads one local skill's current content version as static text for the
/// editor. Binary or oversized files are reported without their bytes.
#[tauri::command]
pub async fn get_skill_editor(
    app: AppHandle,
    definition_id: String,
) -> Result<SkillEditorViewDto, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let definition = store
            .get_definition(&definition_id)
            .map_err(store_error)?
            .ok_or_else(|| CommandError::keyed("extension-not-found", "errors.extlib.extensionNotFound", "扩展不存在或已被删除"))?;
        let ExtensionPayload::Skill(skill) = &definition.payload else {
            return Err(CommandError::keyed("extension-invalid", "errors.extlib.notASkill", "该扩展不是 Skill"));
        };
        let entries = store
            .load_skill_version(&definition.id, &skill.content_digest)
            .map_err(store_error)?;
        let files = entries
            .iter()
            .filter(|entry| entry.kind == asb_core::extensions::validate::ContentEntryKind::File)
            .map(|entry| {
                let text = if entry.bytes.len() <= EDITOR_MAX_TEXT_BYTES {
                    String::from_utf8(entry.bytes.clone()).ok()
                } else {
                    None
                };
                SkillEditorFileDto {
                    relative_path: entry.relative_path.clone(),
                    text,
                    size: entry.size(),
                }
            })
            .collect();
        Ok(SkillEditorViewDto {
            id: definition.id.clone(),
            revision: definition.revision,
            content_digest: skill.content_digest.clone(),
            manifest: skill.manifest.clone(),
            editable: skill.source.is_none() && skill.host_scoped.is_none(),
            files,
        })
    })
    .await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillFileEdit {
    relative_path: String,
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillFilesUpdate {
    expected_revision: u64,
    /// Content digest the editor loaded; a mismatch means another window
    /// published a newer version and the editor must reload.
    expected_digest: String,
    files: Vec<SkillFileEdit>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillDependenciesUpdate {
    expected_revision: u64,
    dependencies: Vec<SkillDependency>,
}

/// Publishes one edited content version for a local skill. Files the editor
/// cannot show are carried over byte for byte; the resulting version is
/// immutable and the previous ones stay recoverable. An unchanged save
/// writes nothing.
#[tauri::command]
pub async fn update_skill_files(
    app: AppHandle,
    definition_id: String,
    update: SkillFilesUpdate,
) -> Result<ExtensionMutationDto, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let mut definition = store
            .get_definition(&definition_id)
            .map_err(store_error)?
            .ok_or_else(|| CommandError::keyed("extension-not-found", "errors.extlib.extensionNotFound", "扩展不存在或已被删除"))?;
        if definition.revision != update.expected_revision {
            return Err(CommandError::keyed(
                "extension-conflict",
                "errors.extlib.skillConflictRefreshEditor",
                "该 Skill 已被其他窗口修改；请刷新编辑器后重试",
            ));
        }
        let ExtensionPayload::Skill(skill) = &definition.payload else {
            return Err(CommandError::keyed("extension-invalid", "errors.extlib.notASkill", "该扩展不是 Skill"));
        };
        if skill.source.is_some() || skill.host_scoped.is_some() {
            return Err(CommandError::keyed(
                "extension-invalid",
                "errors.extlib.scopedSkillNotEditable",
                "带来源或宿主限定的 Skill 不能直接编辑；请先创建本地副本",
            ));
        }
        if skill.content_digest != update.expected_digest {
            return Err(CommandError::keyed(
                "extension-conflict",
                "errors.extlib.contentVersionChangedReload",
                "内容版本已变化；请重新加载编辑器",
            ));
        }
        let previous_revision = definition.revision;
        let current = store
            .load_skill_version(&definition.id, &skill.content_digest)
            .map_err(store_error)?;
        let mut edited: BTreeMap<String, ContentEntry> = BTreeMap::new();
        for file in update.files {
            if file.relative_path.trim().is_empty() {
                return Err(CommandError::keyed("extension-invalid", "errors.extlib.filePathEmpty", "文件路径不能为空"));
            }
            if edited
                .insert(
                    file.relative_path.clone(),
                    ContentEntry {
                        relative_path: file.relative_path,
                        kind: asb_core::extensions::validate::ContentEntryKind::File,
                        bytes: file.text.into_bytes(),
                        mode: 0o644,
                    },
                )
                .is_some()
            {
                return Err(CommandError::keyed(
                    "extension-invalid",
                    "errors.extlib.duplicateFilePath",
                    "保存请求包含重复的文件路径",
                ));
            }
        }
        let mut entries: Vec<ContentEntry> = Vec::new();
        let mut seen_dirs: BTreeSet<String> = BTreeSet::new();
        for entry in &current {
            match entry.kind {
                asb_core::extensions::validate::ContentEntryKind::Dir => {
                    seen_dirs.insert(entry.relative_path.clone());
                    entries.push(entry.clone());
                }
                _ => {
                    if let Some(replacement) = edited.remove(&entry.relative_path) {
                        entries.push(replacement);
                    } else {
                        entries.push(entry.clone());
                    }
                }
            }
        }
        // Newly added files, including their implied parent directories.
        for (relative_path, entry) in edited {
            let mut prefix = String::new();
            let mut segments = relative_path.split('/').collect::<Vec<_>>();
            segments.pop();
            for segment in segments {
                if prefix.is_empty() {
                    prefix.push_str(segment);
                } else {
                    prefix.push('/');
                    prefix.push_str(segment);
                }
                if seen_dirs.insert(prefix.clone())
                    && !entries
                        .iter()
                        .any(|existing| existing.relative_path == prefix)
                {
                    entries.push(ContentEntry {
                        relative_path: prefix.clone(),
                        kind: asb_core::extensions::validate::ContentEntryKind::Dir,
                        bytes: Vec::new(),
                        mode: 0o755,
                    });
                }
            }
            entries.push(entry);
        }
        let skill_md = entries
            .iter()
            .find(|entry| entry.relative_path == "SKILL.md")
            .map(|entry| String::from_utf8_lossy(&entry.bytes).to_string());
        let Some(skill_md) = skill_md else {
            return Err(CommandError::keyed(
                "extension-invalid",
                "errors.extlib.savedContentMissingSkillMd",
                "保存后的内容缺少 SKILL.md",
            ));
        };
        let manifest = match asb_core::extensions::skill::extract_manifest(&skill_md) {
            asb_core::extensions::skill::ManifestExtraction::Parsed(manifest) => manifest,
            _ => {
                return Err(CommandError::keyed(
                    "extension-invalid",
                    "errors.extlib.skillMdFrontmatterUnparsable",
                    "SKILL.md 的 frontmatter 无法解析；需要 name 字段和闭合的 --- 块",
                ))
            }
        };
        asb_core::extensions::skill::deploy_name_for(&manifest)
            .map_err(|error| CommandError::new("extension-invalid", error.message))?;
        let paths: Vec<(
            String,
            asb_core::extensions::validate::ContentEntryKind,
            u64,
        )> = entries
            .iter()
            .map(|entry| (entry.relative_path.clone(), entry.kind, entry.size()))
            .collect();
        asb_core::extensions::validate::validate_content_paths(&paths).map_err(|errors| {
            CommandError::localized(
                "extension-invalid",
                "errors.extlib.skillContentInvalid",
                format!("Skill 内容无效：{}", errors.join("；")),
                serde_json::json!({ "detail": errors.join("；") }),
            )
        })?;
        let digest = asb_core::extensions::skill::content_digest(&entries);
        if digest == skill.content_digest {
            return Ok(extension_mutation(&definition));
        }
        store
            .save_skill_version(&definition.id, &digest, &entries)
            .map_err(store_error)?;
        let ExtensionPayload::Skill(stored_skill) = &mut definition.payload else {
            unreachable!("skill payload checked above");
        };
        stored_skill.content_digest = digest;
        definition.revision += 1;
        definition.updated_at = now();
        store
            .update_definition(&definition, previous_revision)
            .map_err(store_error)?;
        Ok(extension_mutation(&definition))
    })
    .await
}

/// Rewrites one skill's declared dependency links. Links must name existing
/// MCP definitions; the content version is untouched because dependencies
/// live on the definition, not in its immutable content.
#[tauri::command]
pub async fn update_skill_dependencies(
    app: AppHandle,
    definition_id: String,
    update: SkillDependenciesUpdate,
) -> Result<ExtensionMutationDto, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let mut definition = store
            .get_definition(&definition_id)
            .map_err(store_error)?
            .ok_or_else(|| CommandError::keyed("extension-not-found", "errors.extlib.extensionNotFound", "扩展不存在或已被删除"))?;
        if definition.revision != update.expected_revision {
            return Err(CommandError::keyed(
                "extension-conflict",
                "errors.extlib.skillConflictRefreshRetry",
                "该 Skill 已被其他窗口修改；请刷新后重试",
            ));
        }
        for dependency in &update.dependencies {
            let Some(resource_id) = &dependency.resource_id else {
                return Err(CommandError::localized(
                    "extension-invalid",
                    "errors.extlib.dependencyMissingResourceId",
                    format!("依赖 {} 缺少资源标识", dependency.name),
                    serde_json::json!({ "name": dependency.name }),
                ));
            };
            let linked = store
                .get_definition(resource_id)
                .map_err(store_error)?
                .ok_or_else(|| {
                    CommandError::localized(
                        "extension-invalid",
                        "errors.extlib.dependencyNotFound",
                        format!("依赖 {} 不存在", dependency.name),
                        serde_json::json!({ "name": dependency.name }),
                    )
                })?;
            if linked.kind() != ExtensionKind::Mcp {
                return Err(CommandError::localized(
                    "extension-invalid",
                    "errors.extlib.dependencyMustBeMcp",
                    format!("依赖 {} 必须指向 MCP 定义", dependency.name),
                    serde_json::json!({ "name": dependency.name }),
                ));
            }
        }
        let previous_revision = definition.revision;
        let ExtensionPayload::Skill(skill) = &mut definition.payload else {
            return Err(CommandError::keyed("extension-invalid", "errors.extlib.notASkill", "该扩展不是 Skill"));
        };
        skill.dependencies = update.dependencies;
        definition.revision += 1;
        definition.updated_at = now();
        validate_definition(&definition)
            .map_err(|error| CommandError::new("extension-invalid", error.message))?;
        store
            .update_definition(&definition, previous_revision)
            .map_err(store_error)?;
        Ok(extension_mutation(&definition))
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillVersionDto {
    digest: String,
    file_count: u64,
    total_bytes: u64,
    /// Whether this version is the definition's current content.
    is_current: bool,
}

/// Lists a skill's immutable content versions, newest-mutable facts only:
/// digest, size, and which one the definition currently deploys.
#[tauri::command]
pub async fn list_skill_versions(
    app: AppHandle,
    definition_id: String,
) -> Result<Vec<SkillVersionDto>, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let definition = store
            .get_definition(&definition_id)
            .map_err(store_error)?
            .ok_or_else(|| CommandError::keyed("extension-not-found", "errors.extlib.extensionNotFound", "扩展不存在或已被删除"))?;
        let ExtensionPayload::Skill(skill) = &definition.payload else {
            return Err(CommandError::keyed("extension-invalid", "errors.extlib.notASkill", "该扩展不是 Skill"));
        };
        let versions = store
            .list_skill_versions(&definition_id)
            .map_err(store_error)?;
        Ok(versions
            .into_iter()
            .map(|version| SkillVersionDto {
                is_current: version.digest == skill.content_digest,
                digest: version.digest,
                file_count: version.file_count,
                total_bytes: version.total_bytes,
            })
            .collect())
    })
    .await
}
