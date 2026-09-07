use crate::config_store::{
    content_revision, parse_strict, read_optional, write_json_atomic, ConfigStore,
    ProfileStoreError,
};
use asb_core::contracts::{
    AppKind, ProviderDraft, ProviderFile, ProviderProfile, ProviderRecord, RouteMode,
};
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) const POSITION_STEP: u64 = 100;
/// One loaded provider file with its storage revision.
pub(super) struct LoadedProvider {
    pub(super) file: ProviderFile,
    pub(super) hash: String,
}

/// Converts exactly the immediately preceding provider-file contract into the
/// current one. The removed field carried no business meaning once protocol
/// owns credential delivery, so every historical value maps to the same
/// current provider. The canonical file is written before it is exposed to
/// callers; no runtime path consumes both contracts.
fn parse_provider_file(
    app: AppKind,
    path: &Path,
    text: &str,
) -> Result<(ProviderFile, String), ProfileStoreError> {
    if let Ok(file) = parse_strict::<ProviderFile>(text) {
        return Ok((file, text.to_string()));
    }

    let mut value: serde_json::Value =
        serde_json::from_str(text).map_err(|_| ProfileStoreError::Unsupported)?;
    let fields = value
        .as_object_mut()
        .ok_or(ProfileStoreError::Unsupported)?;
    let legacy_authentication = fields
        .remove("authScheme")
        .ok_or(ProfileStoreError::Unsupported)?;
    if !matches!(
        legacy_authentication,
        serde_json::Value::String(ref value)
            if matches!(value.as_str(), "none" | "bearer" | "xApiKey")
    ) {
        return Err(ProfileStoreError::Unsupported);
    }
    let file: ProviderFile =
        serde_json::from_value(value).map_err(|_| ProfileStoreError::Unsupported)?;
    file.clone()
        .into_profile(app)
        .validate()
        .map_err(|_| ProfileStoreError::Unsupported)?;
    if let Some(query) = &file.usage_query {
        crate::usage_query::validate_persisted(query)
            .map_err(|_| ProfileStoreError::Unsupported)?;
    }
    let current = serde_json::to_string_pretty(&file).map_err(|_| ProfileStoreError::Unreadable)?;
    write_json_atomic(path, &current).map_err(|_| ProfileStoreError::Unreadable)?;
    Ok((file, current))
}

fn load_client(
    store: &ConfigStore,
    app: AppKind,
) -> Result<Vec<LoadedProvider>, ProfileStoreError> {
    store.ensure_layout()?;
    let dir = store.providers_dir(app);
    let entries = match fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(_) => return Err(ProfileStoreError::Unreadable),
    };
    let mut loaded = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| ProfileStoreError::Unreadable)?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or(ProfileStoreError::Unsupported)?
            .to_string();
        let text = read_optional(&path)?.ok_or(ProfileStoreError::Unsupported)?;
        let (file, text) = parse_provider_file(app, &path, &text)?;
        if Uuid::parse_str(&file.id).is_err() || file.id != stem {
            return Err(ProfileStoreError::Unsupported);
        }
        let profile = file.clone().into_profile(app);
        profile
            .validate()
            .map_err(|_| ProfileStoreError::Unsupported)?;
        if let Some(query) = &file.usage_query {
            crate::usage_query::validate_persisted(query)
                .map_err(|_| ProfileStoreError::Unsupported)?;
        }
        loaded.push(LoadedProvider {
            file,
            hash: content_revision(text.as_bytes()),
        });
    }
    loaded.sort_by(|left, right| {
        left.file
            .position
            .cmp(&right.file.position)
            .then_with(|| left.file.name.cmp(&right.file.name))
            .then_with(|| left.file.id.cmp(&right.file.id))
    });
    let mut seen_ids = HashSet::new();
    let mut previous_position = 0;
    for provider in &loaded {
        if !seen_ids.insert(provider.file.id.clone()) || provider.file.position <= previous_position
        {
            return Err(ProfileStoreError::Unsupported);
        }
        previous_position = provider.file.position;
    }
    Ok(loaded)
}

/// Loads both provider directories and enforces the globally stable UUID
/// namespace used by every provider command. The directory determines the
/// client, but an id alone remains an unambiguous application identity.
pub(super) fn load_all(
    store: &ConfigStore,
) -> Result<(Vec<LoadedProvider>, Vec<LoadedProvider>), ProfileStoreError> {
    let codex = load_client(store, AppKind::Codex)?;
    let claude = load_client(store, AppKind::Claude)?;
    let mut seen_ids = HashSet::new();
    for provider in codex.iter().chain(&claude) {
        if !seen_ids.insert(provider.file.id.clone()) {
            return Err(ProfileStoreError::Unsupported);
        }
    }
    Ok((codex, claude))
}

pub(super) fn check_expected_files(
    loaded: &[LoadedProvider],
    expected_file_hashes: &BTreeMap<String, String>,
) -> Result<(), String> {
    if loaded.len() != expected_file_hashes.len() {
        return Err("供应商排序版本必须覆盖该客户端的全部供应商".to_string());
    }
    for provider in loaded {
        let expected = expected_file_hashes
            .get(&provider.file.id)
            .ok_or_else(|| "供应商排序版本必须覆盖该客户端的全部供应商".to_string())?;
        if expected != &provider.hash {
            return Err("供应商文件已被外部修改，请重新读取后再排序".to_string());
        }
    }
    Ok(())
}

pub(super) fn record_of(app: AppKind, loaded: &LoadedProvider) -> ProviderRecord {
    ProviderRecord {
        profile: loaded.file.clone().into_profile(app),
        file_hash: loaded.hash.clone(),
    }
}

/// Every validated provider file of one client, in stored order. Snapshot
/// consumers need the files, not the per-file revisions.
pub(crate) fn load_provider_files(
    store: &ConfigStore,
    app: AppKind,
) -> Result<Vec<ProviderFile>, ProfileStoreError> {
    Ok(load_client(store, app)?
        .into_iter()
        .map(|loaded| loaded.file)
        .collect())
}

/// Writes one provider file through the validating, atomic boundary and
/// returns the new storage revision.
pub(super) fn write_provider_file(
    store: &ConfigStore,
    app: AppKind,
    file: &ProviderFile,
) -> Result<String, String> {
    let profile = file.clone().into_profile(app);
    profile.validate().map_err(|error| error.to_string())?;
    if let Some(query) = &file.usage_query {
        crate::usage_query::validate_persisted(query)?;
    }
    let json =
        serde_json::to_string_pretty(file).map_err(|_| "供应商文件序列化失败".to_string())?;
    let path = provider_path(store, app, &file.id);
    write_json_atomic(&path, &json)?;
    Ok(content_revision(json.as_bytes()))
}

pub(super) fn provider_path(store: &ConfigStore, app: AppKind, id: &str) -> PathBuf {
    store.providers_dir(app).join(format!("{id}.json"))
}

pub(super) fn next_position(loaded: &[LoadedProvider]) -> u64 {
    loaded
        .iter()
        .map(|provider| provider.file.position)
        .max()
        .unwrap_or(0)
        + POSITION_STEP
}

/// The routing identity is intentionally narrower than a complete provider:
/// a selected source import may add a missing optional usage query without
/// overwriting the existing provider's notes, website, or configured query.
pub(super) fn same_provider_routing(profile: &ProviderProfile, draft: &ProviderDraft) -> bool {
    if draft.route_mode == RouteMode::Official {
        return profile.app == draft.app && profile.route_mode == RouteMode::Official;
    }
    profile.app == draft.app
        && profile.route_mode == draft.route_mode
        && profile.name == draft.name
        && profile.model == draft.model
        && profile.base_url == draft.base_url
        && profile.api_key == draft.api_key
        && profile.upstream_protocol == draft.upstream_protocol
        && profile.max_output_tokens == draft.max_output_tokens
        && profile.model_options == draft.model_options
}

pub(super) fn same_provider(profile: &ProviderProfile, draft: &ProviderDraft) -> bool {
    same_provider_routing(profile, draft)
        && profile.notes == draft.notes
        && profile.website_url == draft.website_url
        && profile.usage_query == draft.usage_query
}
