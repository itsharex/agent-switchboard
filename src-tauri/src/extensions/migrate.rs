//! One-shot offline migration of the extension library to the current
//! schema. The runtime accepts exactly one schema; when an older library is
//! found it is fully read, converted, written to a staging directory,
//! re-read and verified, then atomically swapped in. A migration never runs
//! while unfinished extension transactions exist, and a crash mid-swap is
//! recovered from the retained pre-migration copy on the next start.

use std::fs;
use std::path::{Path, PathBuf};

use asb_core::extensions::contracts::{
    ExtensionBinding, ExtensionDefinition, ExtensionManifest, ManagedBaselineFile, McpCheckResult,
    OperationSnapshot, ProjectRegistration, EXTENSIONS_SCHEMA_VERSION,
};
use asb_core::extensions::skill::{content_digest, ContentEntry};
use asb_core::extensions::validate::{validate_content_paths, ContentEntryKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Mirrors of the stored metadata shapes, used to verify every migrated
/// file parses as the current schema before the swap.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestFile {
    schema_version: u8,
    generation: u64,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredContentManifest {
    schema_version: u8,
    digest: String,
    files: Vec<StoredFileMirror>,
}

/// The durable receipt format is private to the store module, but it carries
/// the library schema marker and must move with the rest of the library.
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CommitMarkerFile {
    schema_version: u8,
    operation_id: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredFileMirror {
    relative_path: String,
    is_dir: bool,
    mode: u32,
    size: u64,
}

/// Runs the schema migration when the library on disk is older than the
/// current contract. Returns whether a migration was performed. An empty or
/// already-current library is a no-op.
pub fn migrate_library_if_needed(root: &Path) -> Result<bool, String> {
    let manifest_path = root.join("manifest.json");
    if !manifest_path.is_file() {
        return Ok(false);
    }
    let manifest: ManifestFile = read_json(&manifest_path)?;
    if manifest.schema_version == EXTENSIONS_SCHEMA_VERSION {
        return Ok(false);
    }
    if manifest.schema_version != EXTENSIONS_SCHEMA_VERSION - 1 {
        return Err(format!(
            "扩展库版本 {} 无法迁移；仅支持从版本 {} 一次性升级",
            manifest.schema_version,
            EXTENSIONS_SCHEMA_VERSION - 1
        ));
    }
    let transactions = root.join("transactions");
    if transactions.is_dir()
        && fs::read_dir(&transactions)
            .map(|entries| entries.count() > 0)
            .unwrap_or(false)
    {
        return Err("存在未完成的扩展事务；请先启动应用完成恢复，再进行扩展库迁移".to_string());
    }
    perform_migration(root, manifest.generation)
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path).map_err(|error| format!("{}：{error}", path.display()))?;
    serde_json::from_str(&text).map_err(|error| format!("{}：{error}", path.display()))
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("{}：{error}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, json).map_err(|error| format!("{}：{error}", path.display()))
}

/// Lists every JSON metadata file under one subdirectory.
fn json_files(root: &Path, kind: &str) -> Result<Vec<PathBuf>, String> {
    let directory = root.join(kind);
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for entry in fs::read_dir(&directory).map_err(|error| format!("{kind}：{error}"))? {
        let entry = entry.map_err(|error| format!("{kind}：{error}"))?;
        if entry.file_type().map(|t| t.is_file()).unwrap_or(false)
            && entry.file_name().to_string_lossy().ends_with(".json")
        {
            files.push(entry.path());
        }
    }
    Ok(files)
}

fn content_manifests(root: &Path) -> Result<Vec<PathBuf>, String> {
    let library = root.join("library");
    if !library.is_dir() {
        return Ok(Vec::new());
    }
    let mut manifests = Vec::new();
    fn walk(directory: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
        for entry in
            fs::read_dir(directory).map_err(|error| format!("{}：{error}", directory.display()))?
        {
            let entry = entry.map_err(|error| format!("{}：{error}", directory.display()))?;
            if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let manifest = entry.path().join(".asb-content.json");
            if manifest.is_file() {
                out.push(manifest);
            }
            walk(&entry.path(), out)?;
        }
        Ok(())
    }
    walk(&library, &mut manifests)?;
    Ok(manifests)
}

/// Converts one versioned metadata document. A history file is the stored
/// operation snapshot (`record` + `completedSteps` + library states); its v2
/// record restructures from the single-resource shape to the per-resource
/// list, and each completed step gains the owning resource id.
fn convert_document(path: &Path, value: &mut Value) -> Result<(), String> {
    let is_history = path
        .components()
        .any(|component| component.as_os_str() == "history");
    if !value
        .get("schemaVersion")
        .and_then(Value::as_u64)
        .is_some_and(|version| version as u8 == EXTENSIONS_SCHEMA_VERSION - 1)
    {
        return Err(format!("{} 的版本字段不是预期的旧版本", path.display()));
    }
    value["schemaVersion"] = Value::from(EXTENSIONS_SCHEMA_VERSION);
    if is_history {
        let record = value
            .get_mut("record")
            .ok_or_else(|| format!("{} 缺少记录主体", path.display()))?;
        if record.get("schemaVersion").and_then(Value::as_u64)
            != Some(u64::from(EXTENSIONS_SCHEMA_VERSION - 1))
        {
            return Err(format!("{} 的记录版本字段不是预期的旧版本", path.display()));
        }
        record["schemaVersion"] = Value::from(EXTENSIONS_SCHEMA_VERSION);
        // A v2 snapshot's completed steps carry no resource owner; the one
        // v2 resource owns every step of its operation.
        let definition_id = record
            .get("definitionId")
            .cloned()
            .ok_or_else(|| format!("{} 的记录缺少 definitionId", path.display()))?;
        let resource = serde_json::json!({
            "definitionId": definition_id.clone(),
            "definitionRevision": record["definitionRevision"].clone(),
            "operation": record["operation"].clone(),
            "targets": record["targets"].clone(),
        });
        let object = record
            .as_object_mut()
            .ok_or_else(|| format!("{} 的记录不是对象", path.display()))?;
        object.remove("definitionId");
        object.remove("definitionRevision");
        object.remove("operation");
        object.remove("targets");
        object.insert("resources".to_string(), Value::Array(vec![resource]));
        if let Some(steps) = value
            .get_mut("completedSteps")
            .and_then(Value::as_array_mut)
        {
            for step in steps {
                if let Some(step) = step.as_object_mut() {
                    step.entry("resourceId".to_string())
                        .or_insert_with(|| definition_id.clone());
                }
            }
        }
    }
    Ok(())
}

/// Checks the content manifest against its actual immutable tree. A versioned
/// metadata migration must not carry a broken content version into the new
/// schema just because its JSON shape happens to parse.
fn verify_content_manifest(path: &Path, content: &StoredContentManifest) -> Result<(), String> {
    if content.schema_version != EXTENSIONS_SCHEMA_VERSION {
        return Err(format!("{} 迁移后版本不正确", path.display()));
    }
    let paths: Vec<(String, ContentEntryKind, u64)> = content
        .files
        .iter()
        .map(|file| {
            (
                file.relative_path.clone(),
                if file.is_dir {
                    ContentEntryKind::Dir
                } else {
                    ContentEntryKind::File
                },
                file.size,
            )
        })
        .collect();
    validate_content_paths(&paths)
        .map_err(|errors| format!("迁移后校验失败 {}：{}", path.display(), errors.join("；")))?;
    let version_dir = path
        .parent()
        .ok_or_else(|| format!("无法识别 Skill 内容目录 {}", path.display()))?;
    let directory_digest = version_dir
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("无法识别 Skill 内容摘要目录 {}", version_dir.display()))?;
    if directory_digest != content.digest {
        return Err(format!(
            "迁移后校验失败 {}：目录摘要与清单摘要不一致",
            path.display()
        ));
    }
    let mut entries = Vec::new();
    for file in &content.files {
        let target = version_dir.join(&file.relative_path);
        let metadata = fs::symlink_metadata(&target)
            .map_err(|error| format!("迁移后校验失败 {}：{error}", target.display()))?;
        if file.is_dir {
            if !metadata.file_type().is_dir() || file.size != 0 {
                return Err(format!(
                    "迁移后校验失败 {}：内容类型或大小与清单不一致",
                    target.display()
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o7777 != file.mode & 0o7777 {
                    return Err(format!(
                        "迁移后校验失败 {}：权限与清单不一致",
                        target.display()
                    ));
                }
            }
            entries.push(ContentEntry {
                relative_path: file.relative_path.clone(),
                kind: ContentEntryKind::Dir,
                bytes: Vec::new(),
                mode: file.mode,
            });
        } else {
            if !metadata.file_type().is_file() {
                return Err(format!(
                    "迁移后校验失败 {}：内容条目不是普通文件",
                    target.display()
                ));
            }
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if metadata.permissions().mode() & 0o7777 != file.mode & 0o7777 {
                    return Err(format!(
                        "迁移后校验失败 {}：权限与清单不一致",
                        target.display()
                    ));
                }
            }
            let bytes = fs::read(&target)
                .map_err(|error| format!("迁移后校验失败 {}：{error}", target.display()))?;
            if bytes.len() as u64 != file.size {
                return Err(format!(
                    "迁移后校验失败 {}：内容大小与清单不一致",
                    target.display()
                ));
            }
            entries.push(ContentEntry {
                relative_path: file.relative_path.clone(),
                kind: ContentEntryKind::File,
                bytes,
                mode: file.mode,
            });
        }
    }
    if content_digest(&entries) != content.digest {
        return Err(format!("迁移后校验失败 {}：内容摘要不一致", path.display()));
    }
    Ok(())
}

fn verify_current_document(path: &Path, value: Value) -> Result<(), String> {
    if path
        .file_name()
        .is_some_and(|name| name == ".asb-content.json")
    {
        let content: StoredContentManifest = serde_json::from_value(value)
            .map_err(|error| format!("迁移后校验失败 {}：{error}", path.display()))?;
        return verify_content_manifest(path, &content);
    }
    let directory = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .ok_or_else(|| format!("无法识别迁移后文件 {}", path.display()))?;
    match directory {
        "definitions" => {
            let definition: ExtensionDefinition = serde_json::from_value(value)
                .map_err(|error| format!("迁移后校验失败 {}：{error}", path.display()))?;
            asb_core::extensions::validate::validate_definition(&definition)
                .map_err(|error| format!("迁移后校验失败 {}：{}", path.display(), error.message))?;
        }
        "bindings" => {
            let binding: ExtensionBinding = serde_json::from_value(value)
                .map_err(|error| format!("迁移后校验失败 {}：{error}", path.display()))?;
            if binding.schema_version != EXTENSIONS_SCHEMA_VERSION {
                return Err(format!("{} 迁移后版本不正确", path.display()));
            }
        }
        "baselines" => {
            let baseline: ManagedBaselineFile = serde_json::from_value(value)
                .map_err(|error| format!("迁移后校验失败 {}：{error}", path.display()))?;
            if baseline.schema_version != EXTENSIONS_SCHEMA_VERSION {
                return Err(format!("{} 迁移后版本不正确", path.display()));
            }
        }
        "history" => {
            let snapshot: OperationSnapshot = serde_json::from_value(value)
                .map_err(|error| format!("迁移后校验失败 {}：{error}", path.display()))?;
            if snapshot.schema_version != EXTENSIONS_SCHEMA_VERSION
                || snapshot.record.schema_version != EXTENSIONS_SCHEMA_VERSION
                || snapshot
                    .pre_bindings
                    .iter()
                    .chain(snapshot.post_bindings.iter())
                    .any(|binding| binding.schema_version != EXTENSIONS_SCHEMA_VERSION)
                || snapshot
                    .pre_baselines
                    .iter()
                    .chain(snapshot.post_baselines.iter())
                    .any(|(_, baseline)| baseline.schema_version != EXTENSIONS_SCHEMA_VERSION)
            {
                return Err(format!("{} 迁移后版本不正确", path.display()));
            }
        }
        "projects" => {
            let project: ProjectRegistration = serde_json::from_value(value)
                .map_err(|error| format!("迁移后校验失败 {}：{error}", path.display()))?;
            if project.schema_version != EXTENSIONS_SCHEMA_VERSION {
                return Err(format!("{} 迁移后版本不正确", path.display()));
            }
        }
        "commits" => {
            let marker: CommitMarkerFile = serde_json::from_value(value)
                .map_err(|error| format!("迁移后校验失败 {}：{error}", path.display()))?;
            if marker.schema_version != EXTENSIONS_SCHEMA_VERSION
                || marker.operation_id.trim().is_empty()
            {
                return Err(format!("{} 迁移后版本或操作标识不正确", path.display()));
            }
        }
        _ => return Err(format!("无法识别迁移后文件 {}", path.display())),
    }
    Ok(())
}

fn verify_checks(root: &Path) -> Result<(), String> {
    for path in json_files(root, "checks")? {
        let _: Vec<McpCheckResult> = read_json(&path)
            .map_err(|error| format!("迁移后校验失败 {}：{error}", path.display()))?;
    }
    Ok(())
}

fn perform_migration(root: &Path, generation: u64) -> Result<bool, String> {
    // Phase 1: read and convert everything in memory.
    let mut documents: Vec<(PathBuf, Value)> = Vec::new();
    let manifest_path = root.join("manifest.json");
    let mut manifest_value: Value = read_json(&manifest_path)?;
    let manifest: ManifestFile = serde_json::from_value(manifest_value.clone())
        .map_err(|error| format!("清单无效：{error}"))?;
    if manifest.schema_version != EXTENSIONS_SCHEMA_VERSION - 1 {
        return Err("清单版本在迁移期间发生了变化".to_string());
    }
    manifest_value["schemaVersion"] = Value::from(EXTENSIONS_SCHEMA_VERSION);
    manifest_value["generation"] = Value::from(generation + 1);

    for kind in [
        "definitions",
        "bindings",
        "baselines",
        "history",
        "projects",
        "commits",
    ] {
        for path in json_files(root, kind)? {
            let mut value: Value = read_json(&path)?;
            convert_document(&path, &mut value)?;
            documents.push((path, value));
        }
    }
    for path in content_manifests(root)? {
        let mut value: Value = read_json(&path)?;
        let stored: StoredContentManifest = serde_json::from_value(value.clone())
            .map_err(|error| format!("{}：{error}", path.display()))?;
        if stored.schema_version != EXTENSIONS_SCHEMA_VERSION - 1 {
            return Err(format!("{} 的版本字段不是预期的旧版本", path.display()));
        }
        value["schemaVersion"] = Value::from(EXTENSIONS_SCHEMA_VERSION);
        documents.push((path, value));
    }

    // Phase 2: verify every converted document parses as the current schema
    // before anything is written. Check summaries have no schema marker and
    // are copied verbatim, then parsed after staging below.
    for (path, value) in &documents {
        verify_current_document(path, value.clone())?;
    }

    // Phase 3: write the converted tree to a staging directory beside root.
    let staging = root.parent().ok_or("扩展库没有父目录")?.join(format!(
        ".{}.asb-migration-{}",
        root.file_name()
            .ok_or("扩展库目录名无效")?
            .to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&staging).map_err(|error| format!("{}：{error}", staging.display()))?;
    let write_result = (|| -> Result<(), String> {
        // Start from a complete copy so unversioned check summaries and any
        // future current metadata are never silently discarded by a schema
        // migration. Converted documents overwrite their exact counterparts.
        copy_tree(root, &staging)?;
        write_json(&staging.join("manifest.json"), &manifest_value)?;
        for (path, value) in &documents {
            let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
            write_json(&staging.join(relative), value)?;
        }
        Ok(())
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }

    // Phase 4: re-read the staged metadata and verify, then swap atomically.
    let staged_manifest: ExtensionManifest = read_json(&staging.join("manifest.json"))?;
    if staged_manifest.schema_version != EXTENSIONS_SCHEMA_VERSION {
        let _ = fs::remove_dir_all(&staging);
        return Err("暂存清单版本不正确".to_string());
    }
    for (path, _) in &documents {
        let relative = path.strip_prefix(root).map_err(|error| error.to_string())?;
        let staged = staging.join(relative);
        if !staged.is_file() {
            let _ = fs::remove_dir_all(&staging);
            return Err(format!("暂存目录缺少 {}", relative.display()));
        }
        verify_current_document(path, read_json(&staged)?)?;
    }
    verify_checks(&staging)?;
    let backup = root.with_file_name(format!(
        "{}.asb-migration-backup-{}",
        root.file_name()
            .ok_or("扩展库目录名无效")?
            .to_string_lossy(),
        uuid::Uuid::new_v4().simple()
    ));
    fs::rename(root, &backup).map_err(|error| format!("保留旧扩展库失败：{error}"))?;
    if let Err(error) = fs::rename(&staging, root) {
        // Roll the pre-migration copy back into place before failing.
        let _ = fs::rename(&backup, root);
        return Err(format!("切换新扩展库失败：{error}"));
    }
    // The old copy contains no credentials (secrets live in the system
    // store); remove it once the swap succeeded.
    let _ = fs::remove_dir_all(&backup);
    Ok(true)
}

fn copy_tree(source: &Path, target: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(target).map_err(|error| format!("{}：{error}", target.display()))?;
    for entry in fs::read_dir(source).map_err(|error| format!("{}：{error}", source.display()))? {
        let entry = entry.map_err(|error| format!("{}：{error}", source.display()))?;
        let destination = target.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| format!("{}：{error}", entry.path().display()))?;
        if file_type.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &destination)
                .map_err(|error| format!("{}：{error}", destination.display()))?;
        } else {
            return Err(format!(
                "扩展库迁移拒绝非普通文件或目录 {}",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

/// Recovers a migration whose swap was interrupted. A retained pre-migration
/// copy restores a missing live root. A root without a manifest is not
/// deleted or guessed at: it remains intact and blocks writes until the
/// conflict is resolved deliberately.
pub fn recover_interrupted_migration(root: &Path) -> Result<(), String> {
    let parent = root.parent().ok_or("扩展库没有父目录")?;
    if !parent.exists() {
        return Ok(());
    }
    let name = root
        .file_name()
        .ok_or("扩展库目录名无效")?
        .to_string_lossy()
        .to_string();
    let read_dir =
        fs::read_dir(parent).map_err(|error| format!("{}：{error}", parent.display()))?;
    let mut backups = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|error| format!("{}：{error}", parent.display()))?;
        let entry_name = entry.file_name().to_string_lossy().to_string();
        if !entry_name.starts_with(&format!(".{name}.asb-migration-backup-"))
            && !entry_name.starts_with(&format!("{name}.asb-migration-backup-"))
        {
            continue;
        }
        if !entry
            .file_type()
            .map_err(|error| format!("{}：{error}", entry.path().display()))?
            .is_dir()
        {
            return Err(format!(
                "发现名称类似迁移备份的非目录项目 {}；已保留并阻止写入",
                entry.path().display()
            ));
        }
        backups.push(entry.path());
    }
    if backups.is_empty() {
        if root.exists() && !root.join("manifest.json").is_file() {
            return Err("扩展库缺少 manifest.json；为避免覆盖未知数据，已阻止写入".to_string());
        }
        return Ok(());
    }
    if !root.exists() {
        if backups.len() != 1 {
            return Err("发现多个迁移前扩展库备份，无法安全判断要恢复的副本".to_string());
        }
        fs::rename(backups.remove(0), root)
            .map_err(|error| format!("恢复迁移前扩展库失败：{error}"))?;
        return Ok(());
    }
    if !root.join("manifest.json").is_file() {
        return Err(
            "扩展库与迁移备份同时存在但当前库缺少 manifest.json；已保留两者并阻止写入".to_string(),
        );
    }
    // A complete live root means the swap finished; drop only its retained
    // pre-migration copy.
    for backup in backups {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_root() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("extensions");
        fs::create_dir_all(&root).unwrap();
        (directory, root)
    }

    #[test]
    fn migrates_v2_history_and_bumps_every_schema_marker() {
        let (_guard, root) = store_root();
        let empty_digest = content_digest(&[]);
        let content_manifest_path = root
            .join("library")
            .join("ext-1")
            .join(&empty_digest)
            .join(".asb-content.json");
        write_json(
            &root.join("manifest.json"),
            &serde_json::json!({ "schemaVersion": 2, "generation": 7 }),
        )
        .unwrap();
        write_json(
            &root.join("history/op-1.json"),
            &serde_json::json!({
                "schemaVersion": 2,
                "record": {
                    "schemaVersion": 2,
                    "id": "op-1",
                    "operation": "install",
                    "definitionId": "ext-1",
                    "definitionRevision": 1,
                    "createdAt": "2026-09-01T00:00:00Z",
                    "finishedAt": "2026-09-01T00:00:01Z",
                    "targets": [ { "target": { "scope": "app", "client": "codex" }, "outcome": { "kind": "applied" } } ]
                },
                "completedSteps": [ {
                    "target": { "scope": "app", "client": "codex" },
                    "step": { "kind": "documentWritten", "path": "config.toml", "syntax": "toml",
                              "backupReference": null, "writtenHash": "h", "originalHash": null }
                } ],
                "preBaselines": [], "preBindings": [], "postBindings": [], "postBaselines": []
            }),
        )
        .unwrap();
        write_json(
            &content_manifest_path,
            &serde_json::json!({ "schemaVersion": 2, "digest": empty_digest, "files": [] }),
        )
        .unwrap();
        // Check summaries deliberately have no library schema marker. They
        // must survive the tree swap unchanged rather than be mistaken for
        // versioned metadata.
        write_json(
            &root.join("checks/ext-1.json"),
            &serde_json::json!([{
                "definitionId": "ext-1",
                "definitionRevision": 1,
                "target": { "scope": "app", "client": "codex" },
                "checkedAt": "2026-09-01T00:00:00Z",
                "protocolVersion": null,
                "outcome": { "kind": "passed", "tools": 1, "resources": 0, "prompts": 0 },
                "durationMs": 1,
                "truncated": false
            }]),
        )
        .unwrap();
        write_json(
            &root.join("commits/op-previous.json"),
            &serde_json::json!({ "schemaVersion": 2, "operationId": "op-previous" }),
        )
        .unwrap();

        assert!(migrate_library_if_needed(&root).unwrap());
        let manifest: ManifestFile = read_json(&root.join("manifest.json")).unwrap();
        assert_eq!(manifest.schema_version, EXTENSIONS_SCHEMA_VERSION);
        assert_eq!(manifest.generation, 8);
        let history: Value = read_json(&root.join("history/op-1.json")).unwrap();
        assert!(history["record"].get("definitionId").is_none());
        assert_eq!(history["record"]["resources"][0]["definitionId"], "ext-1");
        assert_eq!(history["record"]["resources"][0]["operation"], "install");
        assert_eq!(
            history["completedSteps"][0]["resourceId"], "ext-1",
            "v2 steps gain the owning resource id"
        );
        let content: StoredContentManifest = read_json(&content_manifest_path).unwrap();
        assert_eq!(content.schema_version, EXTENSIONS_SCHEMA_VERSION);
        let checks: Vec<McpCheckResult> = read_json(&root.join("checks/ext-1.json")).unwrap();
        assert_eq!(checks.len(), 1);
        let marker: CommitMarkerFile = read_json(&root.join("commits/op-previous.json")).unwrap();
        assert_eq!(marker.schema_version, EXTENSIONS_SCHEMA_VERSION);
        assert_eq!(marker.operation_id, "op-previous");
        // Running again is a no-op.
        assert!(!migrate_library_if_needed(&root).unwrap());
    }

    #[test]
    fn refuses_to_migrate_a_content_manifest_that_does_not_match_its_tree() {
        let (_guard, root) = store_root();
        write_json(
            &root.join("manifest.json"),
            &serde_json::json!({ "schemaVersion": 2, "generation": 7 }),
        )
        .unwrap();
        let version_dir = root.join("library/ext-1/broken-digest");
        fs::create_dir_all(&version_dir).unwrap();
        fs::write(version_dir.join("SKILL.md"), b"no").unwrap();
        write_json(
            &version_dir.join(".asb-content.json"),
            &serde_json::json!({
                "schemaVersion": 2,
                "digest": "broken-digest",
                "files": [{
                    "relativePath": "SKILL.md",
                    "isDir": false,
                    "mode": 420,
                    "size": 5
                }]
            }),
        )
        .unwrap();

        let error = migrate_library_if_needed(&root).unwrap_err();

        assert!(error.contains("内容大小"));
        let manifest: ManifestFile = read_json(&root.join("manifest.json")).unwrap();
        assert_eq!(manifest.schema_version, EXTENSIONS_SCHEMA_VERSION - 1);
    }

    #[test]
    fn refuses_to_migrate_a_content_manifest_in_the_wrong_digest_directory() {
        let (_guard, root) = store_root();
        write_json(
            &root.join("manifest.json"),
            &serde_json::json!({ "schemaVersion": 2, "generation": 7 }),
        )
        .unwrap();
        let digest = content_digest(&[]);
        write_json(
            &root.join("library/ext-1/not-the-content-digest/.asb-content.json"),
            &serde_json::json!({ "schemaVersion": 2, "digest": digest, "files": [] }),
        )
        .unwrap();

        let error = migrate_library_if_needed(&root).unwrap_err();

        assert!(error.contains("目录摘要"));
        let manifest: ManifestFile = read_json(&root.join("manifest.json")).unwrap();
        assert_eq!(manifest.schema_version, EXTENSIONS_SCHEMA_VERSION - 1);
    }

    #[test]
    fn refuses_to_migrate_with_pending_transactions_or_unknown_versions() {
        let (_guard, root) = store_root();
        write_json(
            &root.join("manifest.json"),
            &serde_json::json!({ "schemaVersion": 1, "generation": 1 }),
        )
        .unwrap();
        let error = migrate_library_if_needed(&root).unwrap_err();
        assert!(error.contains("无法迁移"));

        let (_guard2, root2) = store_root();
        write_json(
            &root2.join("manifest.json"),
            &serde_json::json!({ "schemaVersion": 2, "generation": 1 }),
        )
        .unwrap();
        fs::create_dir_all(root2.join("transactions/op-pending")).unwrap();
        let error = migrate_library_if_needed(&root2).unwrap_err();
        assert!(error.contains("未完成的扩展事务"));
    }

    #[test]
    fn restores_the_only_backup_when_a_swap_lost_the_live_root() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("extensions");
        let backup = directory
            .path()
            .join("extensions.asb-migration-backup-interrupted");
        write_json(
            &backup.join("manifest.json"),
            &serde_json::json!({ "schemaVersion": 2, "generation": 7 }),
        )
        .unwrap();

        recover_interrupted_migration(&root).unwrap();

        assert!(root.join("manifest.json").is_file());
        assert!(!backup.exists());
    }

    #[test]
    fn preserves_an_incomplete_live_root_instead_of_deleting_it_for_a_backup() {
        let (_guard, root) = store_root();
        let backup = root.with_file_name("extensions.asb-migration-backup-interrupted");
        write_json(
            &backup.join("manifest.json"),
            &serde_json::json!({ "schemaVersion": 2, "generation": 7 }),
        )
        .unwrap();
        fs::write(root.join("unrecognized.txt"), "keep").unwrap();

        let error = recover_interrupted_migration(&root).unwrap_err();

        assert!(error.contains("缺少 manifest.json"));
        assert!(root.join("unrecognized.txt").is_file());
        assert!(backup.join("manifest.json").is_file());
    }

    #[test]
    fn refuses_to_restore_from_a_non_directory_backup() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("extensions");
        let backup = directory
            .path()
            .join("extensions.asb-migration-backup-interrupted");
        fs::write(&backup, "not a library").unwrap();

        let error = recover_interrupted_migration(&root).unwrap_err();

        assert!(error.contains("非目录"));
        assert!(!root.exists());
        assert_eq!(fs::read_to_string(backup).unwrap(), "not a library");
    }
}
