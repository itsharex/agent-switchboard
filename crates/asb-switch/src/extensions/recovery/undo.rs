//! Step undo: reverting exactly one completed step, refusing to touch a
//! target whose content is no longer what this transaction wrote.

use std::path::{Path, PathBuf};

use crate::extensions::transaction::walk_content_entries;
use crate::io::{PathKind, SwitchIo};
use asb_core::extensions::contracts::AppliedStep;

/// Undoes exactly one completed step, refusing to touch a target whose
/// content is no longer what this transaction wrote or last observed.
pub(crate) fn undo_step<Io: SwitchIo>(io: &Io, step: &AppliedStep) -> Result<String, String> {
    match step {
        AppliedStep::DocumentWritten {
            path,
            backup_reference,
            written_hash,
            original_hash,
            ..
        } => {
            let path = PathBuf::from(path);
            let current = document_hash(io, &path)?;
            match (original_hash, backup_reference) {
                (Some(original), Some(backup)) => {
                    if current.as_deref() == Some(original.as_str()) {
                        return Ok(format!("{} 已处于恢复前状态", path.display()));
                    }
                    if current.as_deref() != Some(written_hash.as_str()) {
                        return Err(format!(
                            "{} 已不是本事务写入的内容，停止恢复以避免覆盖后续变化",
                            path.display()
                        ));
                    }
                    let backup_path = PathBuf::from(backup);
                    let bytes = io.read_bytes(&backup_path).map_err(|error| {
                        format!("备份 {} 无法读取：{error}", backup_path.display())
                    })?;
                    let restored_hash = crate::extensions::transaction::sha256_bytes_hex(&bytes);
                    if &restored_hash != original {
                        return Err(format!(
                            "备份 {} 与原始内容摘要不一致",
                            backup_path.display()
                        ));
                    }
                    io.write_bytes_replace(&path, &bytes)
                        .map_err(|error| format!("恢复 {} 失败：{error}", path.display()))?;
                    Ok(format!("已恢复 {}", path.display()))
                }
                (None, _) => {
                    if current.is_none() {
                        return Ok(format!("{} 已处于恢复后状态", path.display()));
                    }
                    if current.as_deref() != Some(written_hash.as_str()) {
                        return Err(format!(
                            "{} 已不是本事务写入的内容，停止恢复以避免覆盖后续变化",
                            path.display()
                        ));
                    }
                    io.remove(&path)
                        .map_err(|error| format!("移除 {} 失败：{error}", path.display()))?;
                    Ok(format!("已移除 {}（此前不存在）", path.display()))
                }
                (Some(_), None) => Err(format!("{} 缺少备份引用，无法恢复原内容", path.display())),
            }
        }
        AppliedStep::DocumentRemoved {
            path,
            backup_reference,
            original_hash,
            ..
        } => {
            let path = PathBuf::from(path);
            let current = document_hash(io, &path)?;
            if current.as_deref() == Some(original_hash.as_str()) {
                return Ok(format!("{} 已处于移除前状态", path.display()));
            }
            if current.is_some() {
                return Err(format!(
                    "{} 已不是本事务移除后的状态，停止恢复以避免覆盖后续变化",
                    path.display()
                ));
            }
            let backup_path = PathBuf::from(backup_reference);
            let bytes = io
                .read_bytes(&backup_path)
                .map_err(|error| format!("备份 {} 无法读取：{error}", backup_path.display()))?;
            let restored_hash = crate::extensions::transaction::sha256_bytes_hex(&bytes);
            if &restored_hash != original_hash {
                return Err(format!(
                    "备份 {} 与原始内容摘要不一致",
                    backup_path.display()
                ));
            }
            io.write_bytes_replace(&path, &bytes)
                .map_err(|error| format!("恢复 {} 失败：{error}", path.display()))?;
            Ok(format!("已恢复 {}", path.display()))
        }
        AppliedStep::DirectoryDeployed {
            target_dir,
            backup_reference,
            digest,
            original_digest,
        } => {
            let target = PathBuf::from(target_dir);
            // Idempotent: if the target is already at the original state
            // (or absent when it never existed), there is nothing to undo.
            match walk_content_entries(io, &target) {
                Ok(entries) => {
                    let current_digest = asb_core::extensions::skill::content_digest(&entries);
                    if original_digest.as_deref() == Some(current_digest.as_str()) {
                        return Ok(format!("{} 已处于恢复前状态", target.display()));
                    }
                    if current_digest != *digest {
                        return Err(format!(
                            "{} 已不是本事务部署的内容，停止恢复以避免覆盖后续变化",
                            target.display()
                        ));
                    }
                }
                Err(_) => {
                    // Target vanished; treat as already undone when it was
                    // supposed to be removed by undo anyway.
                    if original_digest.is_none() {
                        return Ok(format!("{} 已处于恢复后状态", target.display()));
                    }
                }
            }
            io.remove_dir_all(&target)
                .map_err(|error| format!("移除部署目录 {} 失败：{error}", target.display()))?;
            match original_digest {
                Some(original) => {
                    let backup = backup_reference.as_deref().ok_or_else(|| {
                        format!("{} 缺少备份引用，无法恢复原目录", target.display())
                    })?;
                    let backup_path = PathBuf::from(backup);
                    crate::extensions::transaction::copy_tree(
                        io,
                        &backup_path,
                        &target,
                        "recovery-copy",
                    )
                    .map_err(|error| error.to_string())?;
                    let restored =
                        walk_content_entries(io, &target).map_err(|error| error.to_string())?;
                    if asb_core::extensions::skill::content_digest(&restored) != *original {
                        return Err(format!("恢复后的 {} 与原始摘要不一致", target.display()));
                    }
                    Ok(format!("已恢复 {}", target.display()))
                }
                None => Ok(format!("已移除 {}（此前不存在）", target.display())),
            }
        }
        AppliedStep::DirectoryRemoved {
            target_dir,
            backup_reference,
            digest,
            ..
        } => {
            let target = PathBuf::from(target_dir);
            match walk_content_entries(io, &target) {
                // Already restored (or an identical twin exists): verify.
                Ok(entries) => {
                    if asb_core::extensions::skill::content_digest(&entries) == *digest {
                        return Ok(format!("{} 已处于移除前状态", target.display()));
                    }
                    Err(format!(
                        "{} 存在但内容与移除前不一致，停止恢复",
                        target.display()
                    ))
                }
                // The removal is still in effect: restore from the backup.
                Err(_) => {
                    let backup_path = PathBuf::from(backup_reference);
                    crate::extensions::transaction::copy_tree(
                        io,
                        &backup_path,
                        &target,
                        "recovery-copy",
                    )
                    .map_err(|error| error.to_string())?;
                    let restored =
                        walk_content_entries(io, &target).map_err(|error| error.to_string())?;
                    if asb_core::extensions::skill::content_digest(&restored) != *digest {
                        return Err(format!("恢复后的 {} 与移除前摘要不一致", target.display()));
                    }
                    Ok(format!("已恢复 {}", target.display()))
                }
            }
        }
    }
}

fn document_hash<Io: SwitchIo>(io: &Io, path: &Path) -> Result<Option<String>, String> {
    match io.path_kind(path).map_err(|error| error.to_string())? {
        PathKind::Absent => Ok(None),
        PathKind::File { .. } => {
            let bytes = io.read_bytes(path).map_err(|error| error.to_string())?;
            Ok(Some(crate::extensions::transaction::sha256_bytes_hex(
                &bytes,
            )))
        }
        PathKind::Directory | PathKind::Other => {
            Err(format!("{} 不是普通文件，无法按文档恢复", path.display()))
        }
    }
}
