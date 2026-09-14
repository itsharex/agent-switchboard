//! Codex-only list and copy operations. Creating a copy never activates it.
use super::{ConfigStore, StoreOperationError};
use asb_core::contracts::CodexProviderRecord;

impl ConfigStore {
    pub(crate) fn duplicate_codex_provider(
        &self,
        id: &str,
        expected_file_hash: &str,
        name: Option<String>,
    ) -> Result<CodexProviderRecord, StoreOperationError> {
        let source = self
            .list_codex_providers()?
            .into_iter()
            .find(|record| record.profile.id == id)
            .ok_or("Codex 供应商不存在")?;
        if source.file_hash != expected_file_hash {
            return Err("Codex 供应商文件已变化，请重新读取后再复制".into());
        }
        let mut draft = source.into_draft();
        draft.name = name.unwrap_or_else(|| format!("{} 副本", draft.name));
        self.create_codex_provider(draft)
    }

    pub(crate) fn search_codex_providers(
        &self,
        query: &str,
    ) -> Result<Vec<CodexProviderRecord>, StoreOperationError> {
        let query = query.trim().to_lowercase();
        Ok(self
            .list_codex_providers()?
            .into_iter()
            .filter(|record| {
                query.is_empty()
                    || [
                        &record.profile.name,
                        &record.profile.endpoint.0,
                        record.notes.as_deref().unwrap_or(""),
                        record.website_url.as_deref().unwrap_or(""),
                    ]
                    .iter()
                    .any(|value| value.to_lowercase().contains(&query))
                    || record
                        .profile
                        .catalog
                        .iter()
                        .any(|model| model.id.to_lowercase().contains(&query))
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copies_all_codex_data_under_an_independent_id_without_changing_the_source() {
        let temporary = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(temporary.path().join("state"));
        let mut draft = asb_core::codex_presets::prepare("codex-preset-02", "test-duplicate-key")
            .unwrap()
            .draft;
        draft.notes = Some("独立副本".into());
        let source = store.create_codex_provider(draft.clone()).unwrap();
        let copy = store
            .duplicate_codex_provider(&source.profile.id, &source.file_hash, Some("Copy".into()))
            .unwrap();
        assert_ne!(copy.profile.id, source.profile.id);
        let mut copied = copy.into_draft();
        copied.name = draft.name.clone();
        assert_eq!(copied, draft);
        let unchanged = store
            .list_codex_providers()
            .unwrap()
            .into_iter()
            .find(|r| r.profile.id == source.profile.id)
            .unwrap();
        assert_eq!(unchanged.file_hash, source.file_hash);
        assert_eq!(store.search_codex_providers("kimi-k3").unwrap().len(), 2);
        assert!(store
            .search_codex_providers("test-duplicate-key")
            .unwrap()
            .is_empty());
        assert!(store
            .duplicate_codex_provider(&source.profile.id, "stale", None)
            .is_err());
        assert!(store
            .duplicate_codex_provider("missing", &source.file_hash, None)
            .is_err());
        assert_eq!(store.list_codex_providers().unwrap().len(), 2);
    }
}
