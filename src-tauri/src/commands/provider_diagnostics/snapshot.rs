use super::{failed, saved_provider};
use crate::{commands::error::{store_error, CommandError}, local_state::LocalState};
use asb_core::{AppKind, BackupRecord, KeyChange};
use std::{collections::BTreeMap, path::{Path, PathBuf}};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Snapshot {
    files: BTreeMap<PathBuf, Option<String>>,
    provider_revision: String,
    settings: String,
    last_write: String,
}

pub(super) fn optional_file(path: &Path) -> Result<Option<String>, CommandError> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CommandError::localized("provider-repair-snapshot-unreadable",
            "providerDiagnostics.backend.snapshotUnreadable", "无法读取配置快照，已拒绝修复或撤回",
            serde_json::json!({ "detail": format!("{}: {error}", path.display()) }))),
    }
}

fn digest<T: serde::Serialize>(value: &T) -> Result<String, CommandError> {
    serde_json::to_string(value).map(|value| asb_switch::sha256_hex(&value))
        .map_err(|error| CommandError::localized("provider-repair-snapshot-unreadable", "providerDiagnostics.backend.snapshotUnreadable",
            "无法读取配置快照，已拒绝修复或撤回", serde_json::json!({ "detail": error.to_string() })))
}

pub(super) fn capture(local: &LocalState, app: AppKind, id: &str, projection: &crate::gateway::GatewayProjection) -> Result<Snapshot, CommandError> {
    let target = local.target(app).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let mut paths = vec![target.clone()];
    if app == AppKind::Codex {
        paths.push(target.with_file_name("auth.json"));
        paths.extend(catalog_path(&target)?);
        if let Some(catalog) = &projection.codex_catalog { paths.push(target.with_file_name(&catalog.file_name)); }
    }
    capture_paths(local, app, id, paths)
}

fn capture_paths(local: &LocalState, app: AppKind, id: &str, paths: Vec<PathBuf>) -> Result<Snapshot, CommandError> {
    let mut files = BTreeMap::new();
    for path in paths {
        files.insert(path.clone(), optional_file(&path)?.map(|text| asb_switch::sha256_hex(&text)));
    }
    let config = local.configuration();
    Ok(Snapshot {
        files, provider_revision: saved_provider(local, id)?.revision,
        settings: digest(&config.get_client_settings(app).map_err(store_error)?)?,
        last_write: digest(&config.latest_config_write(app).map_err(store_error)?)?,
    })
}

fn catalog_path(target: &Path) -> Result<Option<PathBuf>, CommandError> {
    let Some(text) = optional_file(target)? else { return Ok(None); };
    let document = text.parse::<toml_edit::DocumentMut>()
        .map_err(|error| CommandError::new("provider-repair-config-invalid", error.to_string()))?;
    Ok(document.get("model_catalog_json").and_then(|value| value.as_str())
        .filter(|name| name.starts_with("agent-switchboard-codex-") && name.ends_with(".json")
            && Path::new(name).components().count() == 1)
        .map(|name| target.with_file_name(name)))
}

pub(super) fn recapture(local: &LocalState, app: AppKind, id: &str, snapshot: &Snapshot) -> Result<Snapshot, CommandError> {
    capture_paths(local, app, id, snapshot.files.keys().cloned().collect())
}

pub(super) fn unchanged(local: &LocalState, app: AppKind, id: &str, expected: &Snapshot) -> Result<(), CommandError> {
    ensure_snapshot(expected, &recapture(local, app, id, expected)?)
}

fn ensure_snapshot(expected: &Snapshot, actual: &Snapshot) -> Result<(), CommandError> {
    if expected == actual { Ok(()) } else { Err(stale()) }
}

pub(super) fn stale() -> CommandError {
    failed("providerDiagnostics.backend.stale", "配置、认证、模型目录或供应商已变化，请重新诊断；不会覆盖后续修改")
}

pub(super) fn verified_after(local: &LocalState, id: &str, before: &Snapshot, projection: &crate::gateway::GatewayProjection, preview: &asb_switch::FilePreview, final_hash: &str) -> Result<Snapshot, CommandError> {
    let app = projection.plan.app();
    let after = recapture(local, app, id, before)?;
    let target = local.target(app).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let mut expected_files = before.files.clone();
    expected_files.insert(target.clone(), Some(final_hash.into()));
    if let Some(existed) = preview.auth_rendered_existed {
        expected_files.insert(target.with_file_name("auth.json"), if existed { preview.auth_rendered_hash.clone() } else { None });
    }
    if let Some(catalog) = &projection.codex_catalog {
        expected_files.insert(target.with_file_name(&catalog.file_name), Some(asb_switch::sha256_hex(&catalog.content)));
    } else if app == AppKind::Codex && projection.plan.profile.route_mode == asb_core::RouteMode::Official {
        for (path, hash) in &mut expected_files {
            if path.file_name().is_some_and(|name| name.to_string_lossy().starts_with("agent-switchboard-codex-")) { *hash = None; }
        }
    }
    verify_after_files(before, &after, &expected_files)?;
    Ok(after)
}

fn verify_after_files(before: &Snapshot, after: &Snapshot, expected_files: &BTreeMap<PathBuf, Option<String>>) -> Result<(), CommandError> {
    if after.files == *expected_files && after.provider_revision == before.provider_revision && after.settings == before.settings {
        Ok(())
    } else { Err(stale()) }
}

pub(super) fn restore_expectation(local: &LocalState, app: AppKind, snapshot: &Snapshot) -> Result<asb_switch::RestoreExpectation, CommandError> {
    let target = local.target(app).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let content = snapshot.files.get(&target).ok_or_else(stale)?;
    Ok(asb_switch::RestoreExpectation {
        content_hash: content.clone().unwrap_or_else(|| asb_switch::sha256_hex(if app == AppKind::Claude { "{}" } else { "" })),
        target_existed: content.is_some(),
        auth: snapshot.files.get(&target.with_file_name("auth.json")).map(|value| {
            (value.clone().unwrap_or_else(|| asb_switch::sha256_hex("")), value.is_some())
        }),
    })
}

pub(super) fn catalog_change(local: &LocalState, projection: &crate::gateway::GatewayProjection) -> Result<Option<KeyChange>, CommandError> {
    if projection.plan.app() != AppKind::Codex { return Ok(None); }
    let target = local.target(AppKind::Codex).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    if let Some(catalog) = &projection.codex_catalog {
        return catalog_difference(&target.with_file_name(&catalog.file_name), Some(&catalog.content));
    }
    if projection.plan.profile.route_mode == asb_core::RouteMode::Official {
        if let Some(path) = catalog_path(&target)? { return catalog_difference(&path, None); }
    }
    Ok(None)
}

fn catalog_difference(path: &Path, candidate: Option<&str>) -> Result<Option<KeyChange>, CommandError> {
    let current = optional_file(path)?;
    if current.as_deref() == candidate { return Ok(None); }
    if current.is_some() && candidate.is_some() {
        return Err(failed("providerDiagnostics.backend.catalogConflict", "同一修订的模型目录已被修改，执行器拒绝覆盖；请先处理模型目录冲突"));
    }
    Ok(Some(KeyChange {
        key: path.file_name().unwrap_or_default().to_string_lossy().into_owned(),
        kind: if candidate.is_some() { asb_core::ChangeKind::Set } else { asb_core::ChangeKind::Remove },
        before: current.as_ref().map(|value| asb_switch::sha256_hex(value)),
        after: candidate.map(asb_switch::sha256_hex),
    }))
}

pub(super) fn restore_preview(local: &LocalState, gateway: &crate::gateway::GatewayController, record: &BackupRecord) -> Result<(String, Vec<KeyChange>), CommandError> {
    let candidate = super::super::switching::backups::validate_restore_source(local, gateway, record)?;
    let target = local.target(record.app).map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let current = optional_file(&target)?.unwrap_or_default();
    let mut changes = asb_core::adapter::full_diff(record.app, &current, &candidate)
        .map_err(|error| CommandError::new("repair-undo-preview-invalid", error.to_string()))?;
    // full_diff describes the current state relative to its previous argument.
    for change in &mut changes {
        std::mem::swap(&mut change.before, &mut change.after);
        change.kind = if change.after.is_some() { asb_core::ChangeKind::Set } else { asb_core::ChangeKind::Remove };
    }
    let auth = super::super::switching::codex_restore_auth::intent(local, record, &target, &candidate)?;
    if let Some(auth) = &auth {
        if auth.before_hash != auth.after_hash || auth.before_existed != auth.after_existed {
            changes.push(KeyChange {
                key: "auth.json".into(),
                kind: if auth.after_existed { asb_core::ChangeKind::Set } else { asb_core::ChangeKind::Remove },
                before: auth.before_existed.then(|| "[redacted]".into()),
                after: auth.after_existed.then(|| "[redacted]".into()),
            });
        }
    }
    let catalog = if record.app == AppKind::Codex {
        gateway.restored_codex_catalog(local, &candidate)
            .map_err(|error| CommandError::new("backup-catalog-invalid", error))?
    } else { None };
    if let Some(catalog) = &catalog {
        if let Some(change) = catalog_difference(&target.with_file_name(&catalog.file_name), Some(&catalog.content))? { changes.push(change); }
    }
    let fingerprint = digest(&(candidate, auth, catalog.map(|value| value.content)))?;
    Ok((fingerprint, changes))
}

#[cfg(test)]
pub(super) fn fixture() -> Snapshot {
    Snapshot { files: BTreeMap::from([(PathBuf::from("config.toml"), Some("original".into()))]),
        provider_revision: "profile".into(), settings: "settings".into(), last_write: "write".into() }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn preview_snapshots_and_post_repair_receipts_reject_intervening_changes() {
        let before = fixture();
        let mut after = before.clone();
        assert!(ensure_snapshot(&before, &after).is_ok());
        after.files.insert(PathBuf::from("config.toml"), Some("external edit".into()));
        assert!(ensure_snapshot(&before, &after).is_err());
        let expected = BTreeMap::from([(PathBuf::from("config.toml"), Some("repair result".into()))]);
        assert!(verify_after_files(&before, &after, &expected).is_err());
        after.files = expected.clone();
        assert!(verify_after_files(&before, &after, &expected).is_ok());
        after.provider_revision = "edited profile".into();
        assert!(verify_after_files(&before, &after, &expected).is_err());
    }
    #[test]
    fn absent_file_differs_from_empty_and_corrupt_catalog_is_not_overwritten() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("agent-switchboard-codex-test.json");
        assert!(optional_file(&path).unwrap().is_none());
        assert!(catalog_difference(&path, Some("{}" )).unwrap().is_some());
        std::fs::write(&path, "changed").unwrap();
        assert!(catalog_difference(&path, Some("{}" )).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "changed");
    }
}
