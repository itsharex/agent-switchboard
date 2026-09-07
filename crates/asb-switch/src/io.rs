//! Injected filesystem boundary for the switch executor.
//!
//! Production uses [`FsIo`]. Tests wrap `FsIo` with deterministic failure
//! injection so every recovery path can be exercised without touching a real
//! user configuration file.

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

/// What exists at a path, following no links. Reparse points surface as
/// [`PathKind::Other`] so managed operations refuse to touch them instead of
/// following them out of the managed root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathKind {
    Absent,
    File { length: u64, mode: u32 },
    Directory,
    Other,
}

pub trait SwitchIo {
    fn read_file(&self, path: &Path) -> io::Result<String>;
    /// Creates a file that must not already exist (lock acquisition).
    fn write_new_file(&self, path: &Path, content: &str) -> io::Result<()>;
    /// Writes or replaces a file's content in place (backups only).
    fn write_file_replace(&self, path: &Path, content: &str) -> io::Result<()>;
    /// Atomic replace: `from` replaces an existing `to`.
    fn rename_replace(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove(&self, path: &Path) -> io::Result<()>;
    fn ensure_dir(&self, path: &Path) -> io::Result<()>;
    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn now_rfc3339(&self) -> String;

    // --- Extended operations used by the extensions transaction layer. ---

    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>>;
    /// Creates a file that must not already exist, with arbitrary bytes
    /// (staged documents and journal files).
    fn write_new_bytes(&self, path: &Path, content: &[u8]) -> io::Result<()>;
    /// Applies canonical permission bits after creating a managed Skill
    /// entry. Windows accepts the call as a no-op because its file mode
    /// model does not represent Unix execute bits.
    fn set_mode(&self, path: &Path, mode: u32) -> io::Result<()>;
    /// Writes or replaces arbitrary bytes in place (backups, staged copies).
    fn write_bytes_replace(&self, path: &Path, content: &[u8]) -> io::Result<()>;
    /// Existence, type, length, and mode bits of a path, without following
    /// links.
    fn path_kind(&self, path: &Path) -> io::Result<PathKind>;
    /// Plain rename to a path that must not exist (directory swap steps).
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Recursively removes a directory tree (rollback and post-swap cleanup
    /// of transaction-owned directories only).
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Flushes one file's content to stable storage (journal durability).
    fn sync_file(&self, path: &Path) -> io::Result<()>;
    /// Flushes a directory entry change to stable storage.
    fn sync_dir(&self, path: &Path) -> io::Result<()>;
}

pub struct FsIo;

impl SwitchIo for FsIo {
    fn read_file(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }

    fn write_new_file(&self, path: &Path, content: &str) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        use std::io::Write;
        file.write_all(content.as_bytes())
    }

    fn write_file_replace(&self, path: &Path, content: &str) -> io::Result<()> {
        fs::write(path, content)
    }

    fn rename_replace(&self, from: &Path, to: &Path) -> io::Result<()> {
        // std::fs::rename on Windows uses MoveFileEx with
        // MOVEFILE_REPLACE_EXISTING: an atomic replacement of `to`.
        fs::rename(from, to)
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn ensure_dir(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(path)? {
            out.push(entry?.path());
        }
        Ok(out)
    }

    fn now_rfc3339(&self) -> String {
        chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
    }

    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    fn write_new_bytes(&self, path: &Path, content: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        use std::io::Write;
        file.write_all(content)
    }

    fn set_mode(&self, path: &Path, mode: u32) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o7777))
        }
        #[cfg(not(unix))]
        {
            let _ = (path, mode);
            Ok(())
        }
    }

    fn write_bytes_replace(&self, path: &Path, content: &[u8]) -> io::Result<()> {
        fs::write(path, content)
    }

    fn path_kind(&self, path: &Path) -> io::Result<PathKind> {
        match fs::symlink_metadata(path) {
            Ok(metadata) => Ok(path_kind_from_metadata(&metadata)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(PathKind::Absent),
            Err(error) => Err(error),
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn sync_file(&self, path: &Path) -> io::Result<()> {
        // Windows FlushFileBuffers requires a write handle; a read-only
        // open returns access denied.
        OpenOptions::new().write(true).open(path)?.sync_all()
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        #[cfg(unix)]
        {
            std::fs::File::open(path)?.sync_all()
        }
        #[cfg(not(unix))]
        {
            // Windows has no portable directory-handle flush; the journal
            // directory itself is inside the app data tree and the file
            // flush carries the content.
            let _ = path;
            Ok(())
        }
    }
}

#[cfg(unix)]
fn unix_mode(metadata: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn unix_mode(_metadata: &fs::Metadata) -> u32 {
    0o644
}

pub(crate) fn path_kind_from_metadata(metadata: &fs::Metadata) -> PathKind {
    let file_type = metadata.file_type();
    if file_type.is_file() {
        PathKind::File {
            length: metadata.len(),
            mode: unix_mode(metadata),
        }
    } else if file_type.is_dir() {
        PathKind::Directory
    } else {
        PathKind::Other
    }
}
