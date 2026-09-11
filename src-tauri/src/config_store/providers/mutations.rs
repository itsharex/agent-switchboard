use super::load::{
    check_expected_files, load_all, next_position, provider_path, record_of, same_provider,
    same_provider_routing, write_provider_file, LoadedProvider, POSITION_STEP,
};
use crate::config_store::{ConfigStore, ProfileStoreError, StoreOperationError};
use asb_core::contracts::{
    AppKind, ProviderDraft, ProviderFile, ProviderProfile, ProviderRecord, RouteMode,
};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use uuid::Uuid;

impl ConfigStore {
    /// Every provider of the generic store: all Claude providers plus the
    /// Codex official-login records, each group in stored order.
    pub fn list_providers(&self) -> Result<Vec<ProviderRecord>, ProfileStoreError> {
        let (codex, claude) = load_all(self)?;
        let mut records = claude
            .iter()
            .map(|loaded| record_of(AppKind::Claude, loaded))
            .collect::<Vec<_>>();
        records.extend(codex.iter().map(|loaded| record_of(AppKind::Codex, loaded)));
        Ok(records)
    }

    fn locate(&self, id: &str) -> Result<(AppKind, LoadedProvider), StoreOperationError> {
        let (codex, claude) = load_all(self)?;
        if let Some(loaded) = claude.into_iter().find(|loaded| loaded.file.id == id) {
            return Ok((AppKind::Claude, loaded));
        }
        if let Some(loaded) = codex.into_iter().find(|loaded| loaded.file.id == id) {
            return Ok((AppKind::Codex, loaded));
        }
        Err("供应商不存在".into())
    }

    pub fn find_provider(&self, id: &str) -> Result<ProviderProfile, StoreOperationError> {
        let (app, loaded) = self.locate(id)?;
        Ok(loaded.file.into_profile(app))
    }

    /// One provider together with its current storage revision.
    pub fn find_provider_record(&self, id: &str) -> Result<ProviderRecord, StoreOperationError> {
        let (app, loaded) = self.locate(id)?;
        Ok(record_of(app, &loaded))
    }

    /// The exact persisted file of one provider, including its sort
    /// position. The save transaction keeps it as the in-memory pre-image
    /// used to roll a partially applied save back.
    pub fn load_provider_file(
        &self,
        id: &str,
    ) -> Result<(AppKind, ProviderFile), StoreOperationError> {
        let (app, loaded) = self.locate(id)?;
        Ok((app, loaded.file))
    }

    /// Rewrites one provider file through the validating atomic boundary,
    /// preserving its stored position. Only the save transaction's rollback
    /// path calls this, always with a file that was previously persisted.
    pub fn overwrite_provider_file(
        &self,
        app: AppKind,
        file: ProviderFile,
    ) -> Result<String, StoreOperationError> {
        write_provider_file(self, app, &file)
    }

    pub fn create_provider(
        &self,
        draft: ProviderDraft,
    ) -> Result<ProviderRecord, StoreOperationError> {
        if draft.app == AppKind::Codex && draft.route_mode != RouteMode::Official {
            return Err("Codex 第三方供应商必须使用专用档案格式".into());
        }
        draft.validate().map_err(|error| error.to_string())?;
        let (codex, claude) = load_all(self)?;
        let existing = match draft.app {
            AppKind::Codex => codex,
            AppKind::Claude => claude,
        };
        if draft.route_mode == RouteMode::Official
            && existing
                .iter()
                .any(|provider| provider.file.route_mode == RouteMode::Official)
        {
            return Err("该客户端已有官方登录入口".into());
        }
        let profile = ProviderProfile::from_draft(Uuid::new_v4().to_string(), draft);
        let file = ProviderFile::from_profile(&profile, next_position(&existing));
        let hash = write_provider_file(self, profile.app, &file)?;
        Ok(ProviderRecord {
            profile,
            file_hash: hash,
        })
    }

    /// Rewrites exactly one provider file after the optimistic revision
    /// check; the id and sort position are preserved.
    pub fn update_provider(
        &self,
        id: &str,
        draft: ProviderDraft,
        expected_file_hash: &str,
    ) -> Result<ProviderRecord, StoreOperationError> {
        if draft.app == AppKind::Codex && draft.route_mode != RouteMode::Official {
            return Err("Codex 第三方供应商必须使用专用档案格式".into());
        }
        draft.validate().map_err(|error| error.to_string())?;
        let (app, loaded) = self.locate(id)?;
        if app != draft.app {
            return Err("供应商不能变更所属客户端".into());
        }
        if loaded.hash != expected_file_hash {
            return Err("供应商文件已被外部修改，请重新读取后再保存".into());
        }
        if draft.route_mode == RouteMode::Official {
            let (codex, claude) = load_all(self)?;
            let siblings = match app {
                AppKind::Codex => codex,
                AppKind::Claude => claude,
            };
            if siblings.iter().any(|candidate| {
                candidate.file.id != id && candidate.file.route_mode == RouteMode::Official
            }) {
                return Err("该客户端已有官方登录入口".into());
            }
        }
        let profile = ProviderProfile::from_draft(id.to_string(), draft);
        let file = ProviderFile::from_profile(&profile, loaded.file.position);
        let hash = write_provider_file(self, app, &file)?;
        Ok(ProviderRecord {
            profile,
            file_hash: hash,
        })
    }

    pub fn delete_provider(
        &self,
        id: &str,
        expected_file_hash: &str,
    ) -> Result<(), StoreOperationError> {
        let (app, loaded) = self.locate(id)?;
        if loaded.hash != expected_file_hash {
            return Err("供应商文件已被外部修改，请重新读取后再删除".into());
        }
        fs::remove_file(provider_path(self, app, &loaded.file.id))
            .map_err(|_| StoreOperationError::Invalid("无法删除供应商文件".to_string()))
    }

    /// Persists a drag reorder as new `position` values in the affected
    /// provider files. The list must cover the client's providers exactly.
    pub fn reorder_providers(
        &self,
        app: AppKind,
        ordered_ids: &[String],
        expected_file_hashes: &BTreeMap<String, String>,
    ) -> Result<Vec<ProviderRecord>, StoreOperationError> {
        if app == AppKind::Codex {
            return Err("Codex 供应商必须使用专用排序操作".into());
        }
        let (_, claude) = load_all(self)?;
        let loaded = match app {
            AppKind::Codex => unreachable!("Codex uses dedicated provider storage"),
            AppKind::Claude => claude,
        };
        check_expected_files(&loaded, expected_file_hashes)?;
        let unique: HashSet<&str> = ordered_ids.iter().map(String::as_str).collect();
        if unique.len() != ordered_ids.len() || ordered_ids.len() != loaded.len() {
            return Err("排序清单必须覆盖该客户端的全部供应商且不得重复".into());
        }
        // Re-read the complete snapshot and the raw file revisions before
        // constructing the replacement. This rejects a concurrent/manual
        // provider edit (including a formatting-only edit) instead of
        // replacing it blindly.
        let mut snapshot = crate::config_store::snapshot::read_configuration_snapshot(self)?;
        let (codex_after, claude_after) = load_all(self)?;
        let loaded_after = match app {
            AppKind::Codex => codex_after,
            AppKind::Claude => claude_after,
        };
        check_expected_files(&loaded_after, expected_file_hashes)?;
        let current_files: Vec<ProviderFile> =
            loaded_after.into_iter().map(|loaded| loaded.file).collect();
        if snapshot.claude_providers != current_files {
            return Err("供应商文件已被外部修改，请重新读取后再排序".into());
        }
        for id in ordered_ids {
            if !loaded.iter().any(|loaded| &loaded.file.id == id) {
                return Err(format!("排序清单包含不属于该客户端的供应商：{id}").into());
            }
        }
        // A reorder is one logical mutation. Rebuild it through the same
        // verified directory replacement used by restore, so a later file
        // failure cannot leave a subset of positions persisted.
        let files = &mut snapshot.claude_providers;
        for (order, id) in ordered_ids.iter().enumerate() {
            let file = files
                .iter_mut()
                .find(|file| &file.id == id)
                .expect("validated ordering ids exist in the snapshot");
            file.position = (order as u64 + 1) * POSITION_STEP;
        }
        files.sort_by_key(|file| file.position);
        crate::config_store::snapshot::enable_snapshot(self, &snapshot)?;
        Ok(self.list_providers()?)
    }

    /// Imports one draft as a provider file: an exactly equal provider is a
    /// no-op, a routing-identical one gains the missing usage query, and
    /// anything else creates a new file. Codex accepts its official-login
    /// record only; third-party Codex stays in the dedicated store.
    pub fn import_provider(
        &self,
        draft: ProviderDraft,
    ) -> Result<ProviderRecord, StoreOperationError> {
        if draft.app == AppKind::Codex && draft.route_mode != RouteMode::Official {
            return Err("Codex 第三方供应商必须使用专用档案格式".into());
        }
        draft.validate().map_err(|error| error.to_string())?;
        let (codex_official, claude) = load_all(self)?;
        let existing = match draft.app {
            AppKind::Codex => codex_official,
            AppKind::Claude => claude,
        };
        if let Some(loaded) = existing
            .iter()
            .find(|loaded| same_provider(&loaded.file.clone().into_profile(draft.app), &draft))
        {
            return Ok(record_of(draft.app, loaded));
        }
        if let Some(query) = draft.usage_query.clone() {
            let target = existing.iter().find(|loaded| {
                same_provider_routing(&loaded.file.clone().into_profile(draft.app), &draft)
                    && loaded.file.usage_query.is_none()
            });
            if let Some(loaded) = target {
                let mut file = loaded.file.clone();
                file.usage_query = Some(query);
                let hash = write_provider_file(self, draft.app, &file)?;
                return Ok(ProviderRecord {
                    profile: file.into_profile(draft.app),
                    file_hash: hash,
                });
            }
        }
        self.create_provider(draft)
    }

    /// Whether an exactly equal provider already exists (scan-side view of
    /// the import dedup rule).
    pub fn provider_exists(&self, draft: &ProviderDraft) -> bool {
        if draft.app == AppKind::Codex && draft.route_mode != RouteMode::Official {
            return false;
        }
        match load_all(self) {
            Ok((codex, claude)) => {
                let existing = match draft.app {
                    AppKind::Codex => codex,
                    AppKind::Claude => claude,
                };
                existing.iter().any(|loaded| {
                    same_provider(&loaded.file.clone().into_profile(draft.app), draft)
                })
            }
            Err(_) => false,
        }
    }

    /// Whether a selected source import will enrich its otherwise matching
    /// local provider with a currently absent usage query.
    pub fn provider_will_receive_usage_query(&self, draft: &ProviderDraft) -> bool {
        if draft.app == AppKind::Codex && draft.route_mode != RouteMode::Official {
            return false;
        }
        if draft.usage_query.is_none() {
            return false;
        }
        match load_all(self) {
            Ok((codex, claude)) => {
                let existing = match draft.app {
                    AppKind::Codex => codex,
                    AppKind::Claude => claude,
                };
                existing.iter().any(|loaded| {
                    same_provider_routing(&loaded.file.clone().into_profile(draft.app), draft)
                        && loaded.file.usage_query.is_none()
                })
            }
            Err(_) => false,
        }
    }
}
