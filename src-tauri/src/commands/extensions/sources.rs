//! Local skill sources: scanning a directory into cached candidates and
//! importing one candidate (or a discovered native skill/MCP) into the
//! library as host-scoped definitions.

use std::fs;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    ExtensionDefinition, ExtensionKind, ExtensionPayload, ObservedOrigin, SkillDefinition,
    SourceRef, EXTENSIONS_SCHEMA_VERSION,
};
use asb_core::extensions::mcp::{
    import_claude_project_private_server, import_claude_server, import_codex_server,
};
use asb_core::extensions::validate::validate_definition;
use serde::Serialize;
use tauri::AppHandle;

use super::support::*;
use crate::commands::error::{blocking, state, CommandError};
use crate::extensions::discovery::DiscoveredPaths;
use crate::extensions::sources::{self, SkillCandidate};
use crate::extensions::store::ExtensionStore;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillCandidateDto {
    digest: String,
    name: String,
    description: Option<String>,
    file_count: usize,
    diagnostics: Vec<String>,
}

pub(super) fn candidate_dto(candidate: &SkillCandidate) -> SkillCandidateDto {
    SkillCandidateDto {
        digest: candidate.content_digest.clone(),
        name: candidate.name.clone(),
        description: candidate.description.clone(),
        file_count: candidate.entries.len(),
        diagnostics: candidate.diagnostics.clone(),
    }
}

pub(super) fn cache_candidates(list: Vec<SkillCandidate>) {
    let mut cache = candidates().lock().expect("candidates");
    for candidate in list {
        cache.insert(candidate.content_digest.clone(), candidate);
    }
}

#[tauri::command]
pub async fn scan_local_skill_source(
    app: AppHandle,
    root: String,
) -> Result<Vec<SkillCandidateDto>, CommandError> {
    let _ = app;
    blocking(move || {
        let scanned =
            sources::scan_local_source(std::path::Path::new(&root), None).map_err(source_error)?;
        let dtos: Vec<SkillCandidateDto> = scanned.iter().map(candidate_dto).collect();
        cache_candidates(scanned);
        Ok(dtos)
    })
    .await
}

#[tauri::command]
pub async fn resolve_skill_source(
    repo: String,
    subpath: String,
    ref_name: Option<String>,
) -> Result<Vec<SkillCandidateDto>, CommandError> {
    blocking(move || {
        let fetch: sources::HttpFetch = &sources::http_fetch;
        let commit =
            sources::resolve_github_commit(&repo, ref_name.as_deref().unwrap_or("HEAD"), &fetch)
                .map_err(source_error)?;
        let candidate = sources::fetch_github_subtree(&repo, &commit, &subpath, &fetch)
            .map_err(source_error)?;
        let dtos: Vec<SkillCandidateDto> = std::iter::once(&candidate).map(candidate_dto).collect();
        cache_candidates(vec![candidate]);
        Ok(dtos)
    })
    .await
}

/// Imports one cached candidate into the library: the immutable content
/// version first, then a definition pointing at it. Nothing is deployed.
pub(super) fn import_cached_skill_candidate(
    store: &ExtensionStore,
    digest: &str,
    name: String,
    host_scoped: Option<AppKind>,
) -> Result<ExtensionMutationDto, CommandError> {
    let candidate = candidates()
        .lock()
        .expect("candidates")
        .get(digest)
        .cloned()
        .ok_or_else(|| CommandError::new("candidate-expired", "候选内容已过期；请重新扫描来源"))?;
    let manifest_text = candidate
        .entries
        .iter()
        .find(|entry| entry.relative_path == "SKILL.md")
        .map(|entry| String::from_utf8_lossy(&entry.bytes).to_string())
        .unwrap_or_default();
    let manifest = match asb_core::extensions::skill::extract_manifest(&manifest_text) {
        asb_core::extensions::skill::ManifestExtraction::Parsed(manifest) => manifest,
        _ => {
            return Err(CommandError::new(
                "source-rejected",
                "候选内容缺少可解析的 SKILL.md frontmatter",
            ))
        }
    };
    let compatibility = asb_core::extensions::skill::claude_compatibility_notes(&manifest);
    let definition = ExtensionDefinition {
        schema_version: EXTENSIONS_SCHEMA_VERSION,
        id: new_id("ext"),
        name,
        revision: 1,
        created_at: now(),
        updated_at: now(),
        payload: ExtensionPayload::Skill(SkillDefinition {
            content_digest: candidate.content_digest.clone(),
            manifest,
            source: Some(SourceRef {
                source_id: candidate.source_identity.clone(),
                subpath: candidate.subpath.clone(),
                ref_name: None,
                resolved_commit: candidate.resolved_commit.clone(),
            }),
            host_scoped,
            compatibility,
            dependencies: Vec::new(),
        }),
    };
    validate_definition(&definition)
        .map_err(|error| CommandError::new("extension-invalid", error.message))?;
    store
        .save_skill_version(
            &definition.id,
            &candidate.content_digest,
            &candidate.entries,
        )
        .map_err(store_error)?;
    store.create_definition(&definition).map_err(store_error)?;
    Ok(extension_mutation(&definition))
}

#[tauri::command]
pub async fn import_skill_candidate(
    app: AppHandle,
    digest: String,
    name: String,
    host_scoped: Option<AppKind>,
) -> Result<ExtensionMutationDto, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        import_cached_skill_candidate(&store, &digest, name, host_scoped)
    })
    .await
}

/// Imports a currently displayed local Skill without sending its directory
/// path through the renderer. The observation id is invalidated by the next
/// discovery pass; the directory is scanned again and must still match the
/// originally observed content digest.
// Tauri resolves this entrypoint through its generated runtime handler; the
// unit-test crate reaches it only through the HTTP command dispatcher.
#[cfg_attr(test, allow(dead_code))]
#[tauri::command]
pub async fn import_discovered_skill(
    app: AppHandle,
    observation_id: String,
) -> Result<ExtensionMutationDto, CommandError> {
    blocking(move || {
        let observed = discovered_observations()
            .lock()
            .expect("observations")
            .get(&observation_id)
            .cloned()
            .ok_or_else(|| {
                CommandError::new("observation-expired", "发现结果已过期；请重新扫描本机扩展")
            })?
            .observed;
        if observed.kind != ExtensionKind::Skill {
            return Err(CommandError::new(
                "extension-invalid",
                "该发现结果不是 Skill，不能按 Skill 导入",
            ));
        }
        let digest = observed.content_digest.clone().ok_or_else(|| {
            CommandError::new(
                "source-rejected",
                "该 Skill 没有可验证的内容摘要，不能安全导入",
            )
        })?;
        let root = std::path::Path::new(&observed.path)
            .parent()
            .ok_or_else(|| CommandError::new("source-rejected", "发现到的 Skill 路径无效"))?;
        let scanned = sources::scan_local_source(root, None).map_err(source_error)?;
        let candidate = scanned
            .iter()
            .find(|candidate| candidate.content_digest == digest)
            .ok_or_else(|| {
                CommandError::new(
                    "observation-stale",
                    "该 Skill 内容已在扫描后变化；请重新扫描并确认",
                )
            })?
            .clone();
        cache_candidates(scanned);
        let state = state(&app)?;
        let store = extension_store(&state);
        import_cached_skill_candidate(
            &store,
            &candidate.content_digest,
            observed.name,
            Some(observed.client),
        )
    })
    .await
}

/// Imports a discovered native MCP server into the library without exposing
/// its document or raw parameters to the renderer. This copies a definition
/// only; it never claims ownership of the live native entry.
#[allow(dead_code)] // Tauri invokes this command through its generated runtime handler.
#[tauri::command]
pub async fn import_discovered_mcp(
    app: AppHandle,
    observation_id: String,
) -> Result<ExtensionMutationDto, CommandError> {
    blocking(move || {
        let cached = discovered_observations()
            .lock()
            .expect("observations")
            .get(&observation_id)
            .cloned()
            .ok_or_else(|| {
                CommandError::new("observation-expired", "发现结果已过期；请重新扫描本机扩展")
            })?;
        let observed = cached.observed;
        if observed.kind != ExtensionKind::Mcp {
            return Err(CommandError::new(
                "extension-invalid",
                "该发现结果不是 MCP 服务，不能按 MCP 导入",
            ));
        }
        if !observed.managed_binding_ids.is_empty() {
            return Err(CommandError::new(
                "extension-already-managed",
                "该 MCP 服务已由扩展库管理，无需再次导入",
            ));
        }
        let document = fs::read(&observed.path).map_err(|_| {
            CommandError::new(
                "observation-stale",
                "发现到的 MCP 配置已无法读取；请重新扫描并确认",
            )
        })?;
        let document_digest = sha_hex(&document);
        if cached.document_digest.as_deref() != Some(document_digest.as_str()) {
            return Err(CommandError::new(
                "observation-stale",
                "该 MCP 配置已在扫描后变化；请重新扫描并确认",
            ));
        }
        let document = String::from_utf8(document).map_err(|_| {
            CommandError::new("source-rejected", "MCP 配置不是有效文本，不能安全导入")
        })?;
        let discovery_paths = DiscoveredPaths::from_env()
            .map_err(|error| CommandError::new("app-state-unavailable", error))?;
        let claude_user_document = crate::extensions::paths::claude_user_json_path(
            &discovery_paths.home,
            discovery_paths.claude_dir.as_deref(),
        );
        let payload = match (&observed.client, &observed.origin) {
            (AppKind::Claude, ObservedOrigin::ProjectRoot { project_path })
                if std::path::Path::new(&observed.path) == claude_user_document =>
            {
                import_claude_project_private_server(&document, project_path, &observed.name)
            }
            (AppKind::Codex, _) => import_codex_server(&document, &observed.name),
            (AppKind::Claude, _) => import_claude_server(&document, &observed.name),
        }
        .map_err(|error| CommandError::new("source-rejected", error.to_string()))?;

        let state = state(&app)?;
        let store = extension_store(&state);
        if let Some(existing) = store
            .list_definitions()
            .map_err(store_error)?
            .into_iter()
            .find(|definition| {
                definition.name == observed.name
                    && definition.payload == ExtensionPayload::Mcp(payload.clone())
            })
        {
            return Ok(extension_mutation(&existing));
        }
        let definition = ExtensionDefinition {
            schema_version: EXTENSIONS_SCHEMA_VERSION,
            id: new_id("ext"),
            name: observed.name,
            revision: 1,
            created_at: now(),
            updated_at: now(),
            payload: ExtensionPayload::Mcp(payload),
        };
        validate_definition(&definition)
            .map_err(|error| CommandError::new("source-rejected", error.message))?;
        store.create_definition(&definition).map_err(store_error)?;
        Ok(extension_mutation(&definition))
    })
    .await
}
