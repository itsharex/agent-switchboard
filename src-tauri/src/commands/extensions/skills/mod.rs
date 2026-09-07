//! Skill update and authoring: upstream update checks, manifest
//! updates, local skill creation and forking, the file editor with
//! immutable versions, and dependency links.

use std::collections::BTreeMap;

use asb_core::extensions::contracts::{
    ExtensionDefinition, ExtensionPayload, SkillDefinition, SkillManifest,
    EXTENSIONS_SCHEMA_VERSION,
};
use asb_core::extensions::skill::ContentEntry;
use asb_core::extensions::validate::validate_definition;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::commands::error::{blocking, state, CommandError};
use crate::extensions::sources::{self};
use crate::extensions::store::ExtensionStore;

use super::sources::cache_candidates;
use super::support::*;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SkillFileChangeAction {
    Added,
    Removed,
    Modified,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileChange {
    relative_path: String,
    action: SkillFileChangeAction,
}

/// One skill's source-freshness report. Batch checks keep going when a
/// single skill fails; that failure lands in `error` instead of aborting
/// the whole request.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdateReport {
    definition_id: String,
    up_to_date: bool,
    current_commit: Option<String>,
    new_commit: Option<String>,
    new_digest: Option<String>,
    changed_files: Vec<SkillFileChange>,
    error: Option<String>,
}

/// File-level difference between the deployed content version and a fresh
/// source candidate; only regular files participate.
fn skill_file_changes(
    current: &[ContentEntry],
    candidate: &[ContentEntry],
) -> Vec<SkillFileChange> {
    use asb_core::extensions::validate::ContentEntryKind;
    fn files(entries: &[ContentEntry]) -> BTreeMap<&str, &ContentEntry> {
        entries
            .iter()
            .filter(|entry| entry.kind == ContentEntryKind::File)
            .map(|entry| (entry.relative_path.as_str(), entry))
            .collect()
    }
    let (current_files, candidate_files) = (files(current), files(candidate));
    let mut changes = Vec::new();
    for (path, entry) in &candidate_files {
        match current_files.get(path) {
            None => changes.push(SkillFileChange {
                relative_path: (*path).to_string(),
                action: SkillFileChangeAction::Added,
            }),
            Some(existing) if existing.bytes != entry.bytes => {
                changes.push(SkillFileChange {
                    relative_path: (*path).to_string(),
                    action: SkillFileChangeAction::Modified,
                });
            }
            Some(_) => {}
        }
    }
    for path in current_files.keys() {
        if !candidate_files.contains_key(path) {
            changes.push(SkillFileChange {
                relative_path: (*path).to_string(),
                action: SkillFileChangeAction::Removed,
            });
        }
    }
    changes
}

struct SkillUpdateOutcome {
    up_to_date: bool,
    current_commit: Option<String>,
    new_commit: Option<String>,
    new_digest: Option<String>,
    changed_files: Vec<SkillFileChange>,
}

fn check_one_skill_update(
    store: &ExtensionStore,
    definition: &ExtensionDefinition,
) -> Result<SkillUpdateOutcome, CommandError> {
    let ExtensionPayload::Skill(skill) = &definition.payload else {
        return Err(CommandError::new(
            "extension-invalid",
            "更新检查只适用于 Skill",
        ));
    };
    let Some(source) = &skill.source else {
        return Ok(SkillUpdateOutcome {
            up_to_date: true,
            current_commit: None,
            new_commit: None,
            new_digest: None,
            changed_files: Vec::new(),
        });
    };
    let identity = &source.source_id;
    if !identity.contains('/') || source.resolved_commit.is_none() {
        return Ok(SkillUpdateOutcome {
            up_to_date: true,
            current_commit: source.resolved_commit.clone(),
            new_commit: None,
            new_digest: None,
            changed_files: Vec::new(),
        });
    }
    let fetch: sources::HttpFetch = &sources::http_fetch;
    let commit = sources::resolve_github_commit(
        identity,
        source.ref_name.as_deref().unwrap_or("HEAD"),
        &fetch,
    )
    .map_err(source_error)?;
    if commit == source.resolved_commit.clone().unwrap_or_default() {
        return Ok(SkillUpdateOutcome {
            up_to_date: true,
            current_commit: source.resolved_commit.clone(),
            new_commit: Some(commit),
            new_digest: None,
            changed_files: Vec::new(),
        });
    }
    let candidate = sources::fetch_github_subtree(identity, &commit, &source.subpath, &fetch)
        .map_err(source_error)?;
    let up_to_date = candidate.content_digest == skill.content_digest;
    let changed_files = if up_to_date {
        Vec::new()
    } else {
        let current = store
            .load_skill_version(&definition.id, &skill.content_digest)
            .map_err(store_error)?;
        skill_file_changes(&current, &candidate.entries)
    };
    cache_candidates(vec![candidate.clone()]);
    Ok(SkillUpdateOutcome {
        up_to_date,
        current_commit: source.resolved_commit.clone(),
        new_commit: Some(commit),
        new_digest: Some(candidate.content_digest),
        changed_files,
    })
}

/// Checks source freshness for one or more skills in order. Every entry
/// carries its own result; a missing definition or a failed source fetch
/// never removes the other entries.
#[tauri::command]
pub async fn check_skill_updates(
    app: AppHandle,
    definition_ids: Vec<String>,
) -> Result<Vec<SkillUpdateReport>, CommandError> {
    if definition_ids.is_empty() {
        return Err(CommandError::new(
            "extension-invalid",
            "更新检查没有选择任何 Skill",
        ));
    }
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let mut reports = Vec::new();
        for definition_id in &definition_ids {
            let report = match store.get_definition(definition_id).map_err(store_error)? {
                None => SkillUpdateReport {
                    definition_id: definition_id.clone(),
                    up_to_date: true,
                    current_commit: None,
                    new_commit: None,
                    new_digest: None,
                    changed_files: Vec::new(),
                    error: Some("扩展不存在或已被删除".to_string()),
                },
                Some(definition) => match check_one_skill_update(&store, &definition) {
                    Ok(outcome) => SkillUpdateReport {
                        definition_id: definition_id.clone(),
                        up_to_date: outcome.up_to_date,
                        current_commit: outcome.current_commit,
                        new_commit: outcome.new_commit,
                        new_digest: outcome.new_digest,
                        changed_files: outcome.changed_files,
                        error: None,
                    },
                    Err(error) => SkillUpdateReport {
                        definition_id: definition_id.clone(),
                        up_to_date: true,
                        current_commit: None,
                        new_commit: None,
                        new_digest: None,
                        changed_files: Vec::new(),
                        error: Some(error.message),
                    },
                },
            };
            reports.push(report);
        }
        Ok(reports)
    })
    .await
}

/// Updates a skill definition to a freshly cached content version. A digest
/// that already exists in this definition's library history is accepted for
/// version rollback without touching the recorded source commit.
#[tauri::command]
pub async fn update_skill_definition(
    app: AppHandle,
    definition_id: String,
    new_digest: String,
) -> Result<ExtensionMutationDto, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let mut definition = store
            .get_definition(&definition_id)
            .map_err(store_error)?
            .ok_or_else(|| CommandError::new("extension-not-found", "扩展不存在或已被删除"))?;
        // A stored version wins: rolling back to library history must not
        // require a live source scan, and only fresh source candidates may
        // advance the recorded commit. A present-but-corrupt version is a
        // hard load error, never silently treated as an absent one.
        let stored = if store
            .skill_version_exists(&definition.id, &new_digest)
            .map_err(store_error)?
        {
            Some(
                store
                    .load_skill_version(&definition.id, &new_digest)
                    .map_err(store_error)?,
            )
        } else {
            None
        };
        let candidate = if stored.is_some() {
            None
        } else {
            Some(
                candidates()
                    .lock()
                    .expect("candidates")
                    .get(&new_digest)
                    .cloned()
                    .ok_or_else(|| {
                        CommandError::new(
                            "candidate-expired",
                            "候选内容已过期；请重新检查更新或从版本历史选择",
                        )
                    })?,
            )
        };
        let entries = stored
            .or_else(|| {
                candidate
                    .as_ref()
                    .map(|candidate| candidate.entries.clone())
            })
            .expect("entries from one of the two sources");
        let previous_revision = definition.revision;
        let ExtensionPayload::Skill(skill) = &mut definition.payload else {
            return Err(CommandError::new("extension-invalid", "更新只适用于 Skill"));
        };
        store
            .save_skill_version(&definition.id, &new_digest, &entries)
            .map_err(store_error)?;
        skill.content_digest = new_digest.clone();
        if let (Some(source), Some(candidate)) = (&mut skill.source, &candidate) {
            source.resolved_commit = candidate.resolved_commit.clone();
        }
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

// ---------------------------------------------------------- local authoring

/// Upper bound for one file the in-app editor loads as text. Larger or
/// non-UTF-8 files stay in the library untouched and are carried over
/// verbatim on save; the editor never executes anything it shows.
pub(super) const EDITOR_MAX_TEXT_BYTES: usize = 512 * 1024;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalSkillDraft {
    name: String,
    description: String,
}

/// Creates a brand-new local skill from the built-in template. The first
/// content version is published immutably exactly like an imported one.
#[tauri::command]
pub async fn create_local_skill(
    app: AppHandle,
    draft: LocalSkillDraft,
) -> Result<ExtensionMutationDto, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let name = draft.name.trim().to_string();
        let description = draft.description.trim().to_string();
        asb_core::extensions::skill::deploy_name_for(&SkillManifest {
            name: name.clone(),
            description: Some(description.clone()),
            license: None,
            allowed_tools: None,
            unparsed_keys: Vec::new(),
        })
        .map_err(|error| CommandError::new("extension-invalid", error.message))?;
        if description.is_empty() {
            return Err(CommandError::new(
                "extension-invalid",
                "通用 Skill 必须提供 description",
            ));
        }
        let skill_md = format!(
            "---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n\n\
             在这里描述该 Skill 何时触发、需要遵循的步骤与输出约定。\n"
        );
        let entries = vec![ContentEntry {
            relative_path: "SKILL.md".to_string(),
            kind: asb_core::extensions::validate::ContentEntryKind::File,
            bytes: skill_md.clone().into_bytes(),
            mode: 0o644,
        }];
        let manifest = match asb_core::extensions::skill::extract_manifest(&skill_md) {
            asb_core::extensions::skill::ManifestExtraction::Parsed(manifest) => manifest,
            _ => {
                return Err(CommandError::new(
                    "extension-invalid",
                    "模板生成的 SKILL.md 无法解析，请重试",
                ))
            }
        };
        let digest = asb_core::extensions::skill::content_digest(&entries);
        let compatibility = asb_core::extensions::skill::claude_compatibility_notes(&manifest);
        let definition = ExtensionDefinition {
            schema_version: EXTENSIONS_SCHEMA_VERSION,
            id: new_id("ext"),
            name,
            revision: 1,
            created_at: now(),
            updated_at: now(),
            payload: ExtensionPayload::Skill(SkillDefinition {
                content_digest: digest.clone(),
                manifest,
                source: None,
                host_scoped: None,
                compatibility,
                dependencies: Vec::new(),
            }),
        };
        validate_definition(&definition)
            .map_err(|error| CommandError::new("extension-invalid", error.message))?;
        store
            .save_skill_version(&definition.id, &digest, &entries)
            .map_err(store_error)?;
        store.create_definition(&definition).map_err(store_error)?;
        Ok(extension_mutation(&definition))
    })
    .await
}

/// Copies one library skill's current content into a new portable local
/// definition. The source keeps its provenance; the copy is editable in the
/// local workspace and never advances with the source.
#[tauri::command]
pub async fn fork_local_skill(
    app: AppHandle,
    definition_id: String,
) -> Result<ExtensionMutationDto, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let source = store
            .get_definition(&definition_id)
            .map_err(store_error)?
            .ok_or_else(|| CommandError::new("extension-not-found", "扩展不存在或已被删除"))?;
        let ExtensionPayload::Skill(skill) = &source.payload else {
            return Err(CommandError::new(
                "extension-invalid",
                "该扩展不是 Skill，无法创建本地副本",
            ));
        };
        if skill.source.is_none() && skill.host_scoped.is_none() {
            return Err(CommandError::new(
                "extension-invalid",
                "该 Skill 已是本地内容，可直接编辑",
            ));
        }
        let entries = store
            .load_skill_version(&source.id, &skill.content_digest)
            .map_err(store_error)?;
        let compatibility =
            asb_core::extensions::skill::claude_compatibility_notes(&skill.manifest);
        let definition = ExtensionDefinition {
            schema_version: EXTENSIONS_SCHEMA_VERSION,
            id: new_id("ext"),
            name: source.name.clone(),
            revision: 1,
            created_at: now(),
            updated_at: now(),
            payload: ExtensionPayload::Skill(SkillDefinition {
                content_digest: skill.content_digest.clone(),
                manifest: skill.manifest.clone(),
                source: None,
                host_scoped: None,
                compatibility,
                dependencies: skill.dependencies.clone(),
            }),
        };
        validate_definition(&definition).map_err(|error| {
            CommandError::new(
                "extension-invalid",
                format!("该内容缺少通用字段，无法创建本地副本：{}", error.message),
            )
        })?;
        store
            .save_skill_version(&definition.id, &skill.content_digest, &entries)
            .map_err(store_error)?;
        store.create_definition(&definition).map_err(store_error)?;
        Ok(extension_mutation(&definition))
    })
    .await
}

mod editor;

pub use editor::{
    __cmd__get_skill_editor, __cmd__list_skill_versions, __cmd__update_skill_dependencies,
    __cmd__update_skill_files, __tauri_command_name_get_skill_editor,
    __tauri_command_name_list_skill_versions, __tauri_command_name_update_skill_dependencies,
    __tauri_command_name_update_skill_files, get_skill_editor, list_skill_versions,
    update_skill_dependencies, update_skill_files,
};

#[cfg(test)]
mod diff_tests;
