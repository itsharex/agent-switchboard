//! Document backups: byte-exact copies written to the backup directory and
//! verified by read-back before any target is touched.

use std::path::{Path, PathBuf};

use super::ExtensionError;
use crate::executor::timestamp_name;
use crate::io::SwitchIo;

// ---------------------------------------------------------------- backups

pub(super) fn backup_document<Io: SwitchIo>(
    io: &Io,
    path: &Path,
    bytes: &[u8],
    backup_dir: &Path,
    stage: &'static str,
) -> Result<PathBuf, ExtensionError> {
    io.ensure_dir(backup_dir)
        .map_err(|error| ExtensionError::Preparation {
            message: format!(
                "无法创建备份目录 {}（阶段 {stage}）：{error}",
                backup_dir.display()
            ),
        })?;
    let backup_path = backup_dir.join(format!(
        "{}.{}.bak",
        path.file_name()
            .map(|name| name.to_string_lossy())
            .unwrap_or_default(),
        timestamp_name(io)
    ));
    io.write_new_bytes(&backup_path, bytes)
        .map_err(|error| ExtensionError::Preparation {
            message: format!(
                "无法写入备份 {}（阶段 {stage}）：{error}",
                backup_path.display()
            ),
        })?;
    let restored = io
        .read_bytes(&backup_path)
        .map_err(|error| ExtensionError::Preparation {
            message: format!(
                "备份回读失败 {}（阶段 {stage}）：{error}",
                backup_path.display()
            ),
        })?;
    if restored != bytes {
        return Err(ExtensionError::Preparation {
            message: format!("备份回读不一致 {}（阶段 {stage}）", backup_path.display()),
        });
    }
    Ok(backup_path)
}
