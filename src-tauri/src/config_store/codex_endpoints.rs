//! Custom upstream endpoints of a third-party Codex provider (Codex-only).
//!
//! Endpoints are provider metadata inside the specialized Codex file; they
//! never touch the live `config.toml`. Routing reads them once at activation,
//! so the commands refuse to edit the active provider and ask for a re-apply
//! instead. Successful use is recorded from the gateway without a revision
//! guard because it changes scheduling metadata only, never routing identity.

use super::codex_providers::{load, write};
use super::{ConfigStore, StoreOperationError};
use asb_core::contracts::{
    CodexProviderRecord, CodexRouteMode, ProviderConnectionOptions, ProviderEndpoint,
};

impl ConfigStore {
    pub(crate) fn add_codex_endpoint(
        &self,
        id: &str,
        url: &str,
        expected_file_hash: &str,
    ) -> Result<CodexProviderRecord, StoreOperationError> {
        let mut file = self.codex_endpoint_target(id, expected_file_hash)?;
        let normalized = ProviderConnectionOptions::normalize_endpoint_url(url);
        if normalized.is_empty() {
            return Err("Codex 服务端点不能为空".into());
        }
        if normalized == ProviderConnectionOptions::normalize_endpoint_url(&file.profile.endpoint.0)
        {
            return Err("该地址已是 Codex 供应商的主端点".into());
        }
        if find_key(&file.profile.connection, &normalized).is_some() {
            return Err("Codex 服务端点已存在".into());
        }
        file.profile.connection.custom_endpoints.insert(
            normalized.clone(),
            ProviderEndpoint {
                url: normalized,
                added_at: chrono::Utc::now().timestamp_millis(),
                last_used: None,
            },
        );
        reroute(&mut file.profile);
        let hash = write(self, &file)?;
        Ok(file.record(hash))
    }

    pub(crate) fn remove_codex_endpoint(
        &self,
        id: &str,
        url: &str,
        expected_file_hash: &str,
    ) -> Result<CodexProviderRecord, StoreOperationError> {
        let mut file = self.codex_endpoint_target(id, expected_file_hash)?;
        let key = find_key(
            &file.profile.connection,
            &ProviderConnectionOptions::normalize_endpoint_url(url),
        )
        .ok_or_else(|| StoreOperationError::Invalid("Codex 服务端点不存在".to_string()))?;
        file.profile.connection.custom_endpoints.remove(&key);
        reroute(&mut file.profile);
        let hash = write(self, &file)?;
        Ok(file.record(hash))
    }

    /// Records successful use of one custom endpoint. `false` means the URL
    /// is the primary endpoint or is not a configured custom target.
    pub(crate) fn mark_codex_endpoint_used(
        &self,
        id: &str,
        url: &str,
    ) -> Result<bool, StoreOperationError> {
        let Some(mut file) = load(self)?
            .into_iter()
            .find(|loaded| loaded.file.profile.id == id)
            .map(|loaded| loaded.file)
        else {
            return Ok(false);
        };
        let Some(key) = find_key(
            &file.profile.connection,
            &ProviderConnectionOptions::normalize_endpoint_url(url),
        ) else {
            return Ok(false);
        };
        if let Some(endpoint) = file.profile.connection.custom_endpoints.get_mut(&key) {
            endpoint.last_used = Some(chrono::Utc::now().timestamp_millis());
        }
        write(self, &file)?;
        Ok(true)
    }

    fn codex_endpoint_target(
        &self,
        id: &str,
        expected_file_hash: &str,
    ) -> Result<asb_core::contracts::CodexProviderFile, StoreOperationError> {
        let loaded = load(self)?
            .into_iter()
            .find(|loaded| loaded.file.profile.id == id)
            .ok_or_else(|| StoreOperationError::Invalid("Codex 供应商不存在".to_string()))?;
        if loaded.hash != expected_file_hash {
            return Err("Codex 供应商文件已被外部修改，请重新读取后再保存".into());
        }
        Ok(loaded.file)
    }
}

fn find_key(connection: &ProviderConnectionOptions, normalized: &str) -> Option<String> {
    connection
        .custom_endpoints
        .keys()
        .find(|candidate| {
            ProviderConnectionOptions::normalize_endpoint_url(candidate) == normalized
        })
        .cloned()
}

/// Custom endpoints are routing facts: with auto-select on they force the
/// gateway route, so the stored route mode must follow the connection change.
fn reroute(profile: &mut asb_core::contracts::CodexProviderProfile) {
    profile.route_mode = CodexRouteMode::for_connection(
        profile.upstream,
        &profile.connection,
        profile.authentication,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::contracts::{
        CodexCapabilities, CodexCatalogEntry, CodexEndpoint, CodexProviderDraft,
        CodexReasoningLevel, CodexUpstream, ResponsesRequestMode,
    };

    fn draft() -> CodexProviderDraft {
        CodexProviderDraft {
            name: "Relay".into(),
            endpoint: CodexEndpoint("https://relay.example/v1".into()),
            api_key: "secret".into(),
            authentication: None,
            connection: Default::default(),
            upstream: CodexUpstream::Responses,
            request_mode: ResponsesRequestMode::Standard,
            default_model: "codex".into(),
            catalog: vec![CodexCatalogEntry {
                id: "codex".into(),
                context_window: 128_000,
                max_output_tokens: 16_384,
                function_tools: true,
                custom_tools: true,
                tool_search: true,
                reasoning: true,
                default_reasoning_level: CodexReasoningLevel::High,
                supported_reasoning_levels: vec![CodexReasoningLevel::High],
                images: false,
                compact: true,
                display_name: None,
                description: None,
                base_instructions: None,
                supports_parallel_tool_calls: None,
            }],
            model_routes: Vec::new(),
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
            notes: Some("keep".into()),
            website_url: None,
            usage_query: None,
        }
    }

    #[test]
    fn codex_endpoints_are_guarded_by_revision_and_never_duplicate_the_primary() {
        let directory = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(directory.path().join("state"));
        let created = store.create_codex_provider(draft()).unwrap();
        let id = created.profile.id.clone();

        assert!(store
            .add_codex_endpoint(&id, "https://backup.example/v1", "stale")
            .is_err());
        assert!(store
            .add_codex_endpoint(&id, "https://relay.example/v1/", &created.file_hash)
            .unwrap_err()
            .to_string()
            .contains("主端点"));
        assert!(store
            .add_codex_endpoint(&id, "not a url", &created.file_hash)
            .is_err());

        let added = store
            .add_codex_endpoint(&id, "https://backup.example/v1/", &created.file_hash)
            .unwrap();
        let endpoints = &added.profile.connection.custom_endpoints;
        assert_eq!(endpoints.len(), 1);
        assert_eq!(
            endpoints["https://backup.example/v1"].url,
            "https://backup.example/v1"
        );
        assert_eq!(added.notes.as_deref(), Some("keep"), "无关字段原样保留");
        assert!(store
            .add_codex_endpoint(&id, "https://backup.example/v1", &added.file_hash)
            .unwrap_err()
            .to_string()
            .contains("已存在"));

        assert!(store
            .mark_codex_endpoint_used(&id, "https://relay.example/v1")
            .map(|used| !used)
            .unwrap());
        assert!(store
            .mark_codex_endpoint_used(&id, "https://backup.example/v1/")
            .unwrap());
        let (after_use, _) = store.find_codex_provider_with_revision(&id).unwrap();
        assert!(
            after_use.profile.connection.custom_endpoints["https://backup.example/v1"]
                .last_used
                .is_some()
        );
        assert_eq!(
            after_use
                .profile
                .connection
                .endpoint_candidates(&after_use.profile.endpoint.0),
            vec!["https://relay.example/v1", "https://backup.example/v1"],
            "主端点始终第一，其后按最近使用排序"
        );

        let (_, revision) = store.find_codex_provider_with_revision(&id).unwrap();
        assert!(store
            .remove_codex_endpoint(&id, "https://missing.example", &revision)
            .is_err());
        let removed = store
            .remove_codex_endpoint(&id, "https://backup.example/v1", &revision)
            .unwrap();
        assert!(removed.profile.connection.custom_endpoints.is_empty());
    }
}
