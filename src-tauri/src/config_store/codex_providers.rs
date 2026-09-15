//! Strict persistence boundary for third-party Codex provider files.
//!
//! Codex files intentionally do not share Claude's `ProviderFile` schema.
//! A file that predates this boundary is rejected rather than reinterpreted.

use crate::config_store::{
    content_revision, parse_strict, read_optional, write_json_atomic, ConfigStore,
    ProfileStoreError, StoreOperationError,
};
use asb_core::contracts::{
    CodexEndpoint, CodexProviderDraft, CodexProviderFile, CodexProviderRecord, CodexUpstream,
    UsageQuery, CODEX_PROVIDER_SCHEMA_VERSION,
};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use uuid::Uuid;

pub(super) struct LoadedCodexProvider {
    pub(super) file: CodexProviderFile,
    pub(super) hash: String,
}

impl ConfigStore {
    /// Every third-party Codex profile in stored order. Only the current
    /// Codex-specific file shape is accepted from this boundary.
    pub fn list_codex_providers(&self) -> Result<Vec<CodexProviderRecord>, ProfileStoreError> {
        Ok(load(self)?
            .into_iter()
            .map(|loaded| loaded.file.record(loaded.hash))
            .collect())
    }

    /// Creates one fully typed Codex provider file. The generic provider
    /// mutation API never writes into the Codex directory.
    pub fn create_codex_provider(
        &self,
        draft: CodexProviderDraft,
    ) -> Result<CodexProviderRecord, StoreOperationError> {
        let loaded = load(self)?;
        let position = loaded
            .iter()
            .map(|provider| provider.file.position)
            .max()
            .unwrap_or(0)
            .checked_add(crate::config_store::PROVIDER_POSITION_STEP)
            .ok_or_else(|| StoreOperationError::Invalid("Codex 供应商排序位置溢出".to_string()))?;
        let file = draft.into_file(Uuid::new_v4().to_string(), position);
        let hash = write(self, &file)?;
        Ok(file.record(hash))
    }

    /// Whether a stored provider already routes through the seed's endpoint
    /// and upstream. A scan-only hint: completing the seed in the editor
    /// stays allowed even when a matching route exists.
    pub fn codex_route_exists(&self, endpoint: &CodexEndpoint, upstream: CodexUpstream) -> bool {
        load(self)
            .map(|loaded| {
                loaded.iter().any(|existing| {
                    existing.file.profile.endpoint == *endpoint
                        && existing.file.profile.upstream == upstream
                })
            })
            .unwrap_or(false)
    }

    /// Whether the stored provider routing through this endpoint and
    /// upstream already carries a usage query (scan-side input for the
    /// import hint).
    pub fn codex_route_has_usage_query(
        &self,
        endpoint: &CodexEndpoint,
        upstream: CodexUpstream,
    ) -> bool {
        load(self)
            .map(|loaded| {
                loaded.iter().any(|existing| {
                    existing.file.profile.endpoint == *endpoint
                        && existing.file.profile.upstream == upstream
                        && existing.file.usage_query.is_some()
                })
            })
            .unwrap_or(false)
    }

    /// Delivers one source usage query to the stored provider routing
    /// through this endpoint and upstream when it has none yet — the strict
    /// store's mirror of the generic import boundary's enrichment. Returns
    /// whether a profile changed; a missing route or an existing query
    /// leaves everything untouched.
    pub fn enrich_codex_route_usage_query(
        &self,
        endpoint: &CodexEndpoint,
        upstream: CodexUpstream,
        query: UsageQuery,
    ) -> Result<bool, StoreOperationError> {
        let Some(mut file) = load(self)?
            .into_iter()
            .find(|loaded| {
                loaded.file.profile.endpoint == *endpoint
                    && loaded.file.profile.upstream == upstream
            })
            .map(|loaded| loaded.file)
        else {
            return Ok(false);
        };
        if file.usage_query.is_some() {
            return Ok(false);
        }
        file.usage_query = Some(query);
        write(self, &file).map(|_| true)
    }

    pub fn update_codex_provider(
        &self,
        id: &str,
        draft: CodexProviderDraft,
        expected_file_hash: &str,
    ) -> Result<CodexProviderRecord, StoreOperationError> {
        let existing = load(self)?
            .into_iter()
            .find(|loaded| loaded.file.profile.id == id)
            .ok_or_else(|| StoreOperationError::Invalid("Codex 供应商不存在".to_string()))?;
        if existing.hash != expected_file_hash {
            return Err("Codex 供应商文件已被外部修改，请重新读取后再保存".into());
        }
        let file = draft.into_file(id.to_string(), existing.file.position);
        let hash = write(self, &file)?;
        Ok(file.record(hash))
    }

    /// Rewrites one already materialized Codex file after an optimistic
    /// revision check. Switching uses this for live backfill because the
    /// candidate must preserve every specialized field that was not present
    /// in `config.toml`.
    pub(crate) fn update_codex_provider_file(
        &self,
        file: CodexProviderFile,
        expected_file_hash: &str,
    ) -> Result<CodexProviderRecord, StoreOperationError> {
        let existing = load(self)?
            .into_iter()
            .find(|loaded| loaded.file.profile.id == file.profile.id)
            .ok_or_else(|| StoreOperationError::Invalid("Codex 供应商不存在".to_string()))?;
        if existing.hash != expected_file_hash {
            return Err("Codex 供应商文件已被外部修改，请重新读取后再保存".into());
        }
        if existing.file.position != file.position {
            return Err("Codex 供应商排序位置已变化，拒绝覆盖".into());
        }
        let hash = write(self, &file)?;
        Ok(file.record(hash))
    }

    /// Restores a previously validated Codex provider preimage. This is
    /// deliberately crate-visible: only the active-save recovery transaction
    /// may overwrite a specialized provider file.
    pub(crate) fn overwrite_codex_provider_file(
        &self,
        file: CodexProviderFile,
    ) -> Result<(), StoreOperationError> {
        write(self, &file).map(|_| ())
    }

    pub fn delete_codex_provider(
        &self,
        id: &str,
        expected_file_hash: &str,
    ) -> Result<(), StoreOperationError> {
        let existing = load(self)?
            .into_iter()
            .find(|loaded| loaded.file.profile.id == id)
            .ok_or_else(|| StoreOperationError::Invalid("Codex 供应商不存在".to_string()))?;
        if existing.hash != expected_file_hash {
            return Err("Codex 供应商文件已被外部修改，请重新读取后再删除".into());
        }
        let path = self
            .providers_dir(asb_core::contracts::AppKind::Codex)
            .join(format!("{id}.json"));
        fs::remove_file(path)
            .map_err(|_| StoreOperationError::Invalid("无法删除 Codex 供应商文件".to_string()))
    }

    pub fn reorder_codex_providers(
        &self,
        ordered_ids: &[String],
        expected_file_hashes: &BTreeMap<String, String>,
    ) -> Result<Vec<CodexProviderRecord>, StoreOperationError> {
        let loaded = load(self)?;
        if loaded.len() != ordered_ids.len() || loaded.len() != expected_file_hashes.len() {
            return Err("Codex 排序版本必须覆盖全部供应商".into());
        }
        let known = loaded
            .iter()
            .map(|item| (item.file.profile.id.as_str(), item.hash.as_str()))
            .collect::<BTreeMap<_, _>>();
        let unique = ordered_ids.iter().collect::<HashSet<_>>();
        if unique.len() != ordered_ids.len()
            || ordered_ids.iter().any(|id| {
                known.get(id.as_str()).is_none_or(|hash| {
                    expected_file_hashes
                        .get(id)
                        .is_none_or(|expected| *hash != expected)
                })
            })
        {
            return Err("Codex 供应商文件已被外部修改，请重新读取后再排序".into());
        }
        let mut snapshot = crate::config_store::snapshot::read_configuration_snapshot(self)?;
        for (index, id) in ordered_ids.iter().enumerate() {
            let file = snapshot
                .codex_providers
                .iter_mut()
                .find(|file| &file.profile.id == id)
                .ok_or_else(|| {
                    StoreOperationError::Invalid("Codex 排序清单包含未知供应商".to_string())
                })?;
            file.position = (index as u64 + 1) * crate::config_store::PROVIDER_POSITION_STEP;
        }
        crate::config_store::snapshot::enable_snapshot(self, &snapshot)?;
        Ok(self.list_codex_providers()?)
    }
}

/// Raw Codex files for boundaries that must preserve the specialized
/// persistence contract, including configuration snapshots and recovery.
pub(crate) fn load_codex_provider_files(
    store: &ConfigStore,
) -> Result<Vec<CodexProviderFile>, ProfileStoreError> {
    Ok(load(store)?.into_iter().map(|loaded| loaded.file).collect())
}

/// Finds one current Codex file without converting it into the retired
/// generic provider shape.
impl ConfigStore {
    pub(crate) fn codex_provider_snapshots(
        &self,
    ) -> Result<Vec<(CodexProviderFile, String)>, ProfileStoreError> {
        Ok(load(self)?
            .into_iter()
            .map(|loaded| (loaded.file, loaded.hash))
            .collect())
    }

    pub(crate) fn find_codex_provider_with_revision(
        &self,
        id: &str,
    ) -> Result<(CodexProviderFile, String), ProfileStoreError> {
        load(self)?
            .into_iter()
            .find(|loaded| loaded.file.profile.id == id)
            .map(|loaded| (loaded.file, loaded.hash))
            .ok_or(ProfileStoreError::Unsupported)
    }

    pub(crate) fn find_codex_provider_file(
        &self,
        id: &str,
    ) -> Result<CodexProviderFile, ProfileStoreError> {
        load(self)?
            .into_iter()
            .find(|loaded| loaded.file.profile.id == id)
            .map(|loaded| loaded.file)
            .ok_or(ProfileStoreError::Unsupported)
    }
}

pub(super) fn load(store: &ConfigStore) -> Result<Vec<LoadedCodexProvider>, ProfileStoreError> {
    store.ensure_layout()?;
    let directory = store.providers_dir(asb_core::contracts::AppKind::Codex);
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(_) => return Err(ProfileStoreError::Unreadable),
    };
    let mut loaded = Vec::new();
    for entry in entries {
        let path = entry.map_err(|_| ProfileStoreError::Unreadable)?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or(ProfileStoreError::Unsupported)?;
        let text = read_optional(&path)?.ok_or(ProfileStoreError::Unsupported)?;
        let file: CodexProviderFile = parse_strict(&text)?;
        if file.schema_version != CODEX_PROVIDER_SCHEMA_VERSION
            || Uuid::parse_str(&file.profile.id).is_err()
            || file.profile.id != stem
            || file.validate().is_err()
        {
            return Err(ProfileStoreError::Unsupported);
        }
        loaded.push(LoadedCodexProvider {
            file,
            hash: content_revision(text.as_bytes()),
        });
    }
    loaded.sort_by(|left, right| {
        left.file
            .position
            .cmp(&right.file.position)
            .then_with(|| left.file.profile.name.cmp(&right.file.profile.name))
            .then_with(|| left.file.profile.id.cmp(&right.file.profile.id))
    });
    let mut ids = HashSet::new();
    let mut previous_position = 0;
    for provider in &loaded {
        if !ids.insert(provider.file.profile.id.as_str())
            || provider.file.position <= previous_position
        {
            return Err(ProfileStoreError::Unsupported);
        }
        previous_position = provider.file.position;
    }
    Ok(loaded)
}

pub(super) fn write(
    store: &ConfigStore,
    file: &CodexProviderFile,
) -> Result<String, StoreOperationError> {
    file.validate()?;
    let json =
        serde_json::to_string_pretty(file).map_err(|_| "Codex 供应商文件序列化失败".to_string())?;
    store.initialize_current_layout()?;
    let path = store
        .providers_dir(asb_core::contracts::AppKind::Codex)
        .join(format!("{}.json", file.profile.id));
    write_json_atomic(&path, &json)?;
    Ok(content_revision(json.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::contracts::{
        CodexCapabilities, CodexCatalogEntry, CodexEndpoint, CodexModelRoute, CodexUpstream,
    };

    fn draft() -> CodexProviderDraft {
        CodexProviderDraft {
            name: "Relay".to_string(),
            endpoint: CodexEndpoint("https://relay.example/v1".to_string()),
            api_key: "secret".to_string(),
            authentication: None,
            connection: Default::default(),
            upstream: CodexUpstream::Responses,
            request_mode: asb_core::contracts::ResponsesRequestMode::Standard,
            default_model: "codex".to_string(),
            catalog: vec![CodexCatalogEntry {
                id: "codex".to_string(),
                context_window: 128_000,
                max_output_tokens: 16_384,
                function_tools: true,
                custom_tools: true,
                tool_search: true,
                reasoning: true,
                default_reasoning_level: asb_core::contracts::CodexReasoningLevel::High,
                supported_reasoning_levels: vec![
                    asb_core::contracts::CodexReasoningLevel::None,
                    asb_core::contracts::CodexReasoningLevel::High,
                ],
                images: false,
                compact: true,
                display_name: None,
                description: None,
                base_instructions: None,
                supports_parallel_tool_calls: None,
            }],
            model_routes: vec![CodexModelRoute {
                client_model: "codex".to_string(),
                upstream_model: "vendor-codex".to_string(),
            }],
            capabilities: CodexCapabilities {
                responses: true,
                compact: true,
                models: true,
                chat_completions: false,
                alpha_search: false,
                image_generation: false,
                image_edit: false,
                function_tools: true,
                custom_tools: true,
                tool_search: true,
                reasoning: true,
                chat_reasoning: asb_core::contracts::CodexChatReasoning::Unsupported,
            },
            parameters: asb_core::ownership::default_provider_parameters(
                asb_core::contracts::AppKind::Codex,
            ),
            notes: None,
            website_url: None,
            usage_query: None,
        }
    }

    #[test]
    fn codex_store_writes_and_reads_only_the_specialized_file() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(directory.path().join("state"));
        let created = store.create_codex_provider(draft()).unwrap();
        let listed = store.list_codex_providers().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].profile, created.profile);
    }

    #[test]
    fn codex_route_exists_marks_a_stored_route_without_blocking_completion() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(directory.path().join("state"));
        let created = store.create_codex_provider(draft()).unwrap();
        assert!(store.codex_route_exists(&created.profile.endpoint, created.profile.upstream));
        assert!(!store.codex_route_exists(
            &CodexEndpoint("https://other.example/v1".to_string()),
            created.profile.upstream
        ));
        assert!(
            !store.codex_route_exists(&created.profile.endpoint, CodexUpstream::ChatCompletions)
        );
    }

    #[test]
    fn codex_store_rejects_the_old_generic_file_shape() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(directory.path().join("state"));
        let path = store
            .providers_dir(asb_core::contracts::AppKind::Codex)
            .join("00000000-0000-0000-0000-000000000001.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            r#"{"id":"00000000-0000-0000-0000-000000000001","model":null}"#,
        )
        .unwrap();
        assert_eq!(
            store.list_codex_providers(),
            Err(ProfileStoreError::Unsupported)
        );
    }

    #[test]
    fn codex_store_updates_only_the_expected_revision_and_preserves_position() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(directory.path().join("state"));
        let created = store.create_codex_provider(draft()).unwrap();
        let mut changed = draft();
        changed.name = "Updated relay".to_string();
        let saved = store
            .update_codex_provider(&created.profile.id, changed, &created.file_hash)
            .unwrap();
        assert_eq!(saved.profile.name, "Updated relay");
        assert_ne!(saved.file_hash, created.file_hash);
        assert!(store
            .update_codex_provider(&saved.profile.id, draft(), &created.file_hash)
            .is_err());
    }

    #[test]
    fn codex_store_level_failures_stay_typed() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(directory.path().join("state"));
        std::fs::create_dir_all(store.legacy_store_path().parent().unwrap()).unwrap();
        std::fs::write(store.legacy_store_path(), b"{}").unwrap();
        assert_eq!(
            store.create_codex_provider(draft()),
            Err(StoreOperationError::Store(ProfileStoreError::Unsupported))
        );
    }

    #[test]
    fn codex_store_deletes_only_the_expected_revision() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(directory.path().join("state"));
        let created = store.create_codex_provider(draft()).unwrap();
        assert!(store
            .delete_codex_provider(&created.profile.id, "stale")
            .is_err());
        store
            .delete_codex_provider(&created.profile.id, &created.file_hash)
            .unwrap();
        assert!(store.list_codex_providers().unwrap().is_empty());
    }
}
