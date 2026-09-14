use std::collections::BTreeMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;

use asb_core::extensions::skill::ContentEntry;
use asb_core::extensions::validate::{ContentEntryKind, MAX_CONTENT_FILE_BYTES};

use super::github::{relative_to, validate_subpath};
use super::transport::MAX_ARCHIVE_DOWNLOAD;
use super::zip_header::validate_zip_directory;
use super::{
    check_candidate_totals, finish_candidate, SkillCandidate, SourceError, MAX_SOURCE_BYTES,
    MAX_SOURCE_CANDIDATES,
};

/// ZIP provenance is explicitly local (zip:<canonical path>), with no commit.
/// Existing remote update checks therefore never treat it as a GitHub repo.
pub fn scan_zip_source(path: &Path) -> Result<Vec<SkillCandidate>, SourceError> {
    let metadata = fs::symlink_metadata(path).map_err(unreachable)?;
    #[cfg(windows)]
    let reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let reparse = false;
    if !metadata.is_file() || metadata.file_type().is_symlink() || reparse {
        return Err(rejected(
            "source must be a regular ZIP file, not a link or reparse point",
        ));
    }
    if metadata.len() > MAX_ARCHIVE_DOWNLOAD {
        return Err(rejected("archive exceeds the 64 MiB input limit"));
    }
    let canonical = fs::canonicalize(path).map_err(unreachable)?;
    let mut bytes = Vec::new();
    fs::File::open(&canonical)
        .map_err(unreachable)?
        .take(MAX_ARCHIVE_DOWNLOAD + 1)
        .read_to_end(&mut bytes)
        .map_err(unreachable)?;
    if bytes.len() as u64 > MAX_ARCHIVE_DOWNLOAD {
        return Err(rejected("archive exceeded the input limit while reading"));
    }
    let entries = read_archive(&bytes)?;
    archive_candidates(format!("zip:{}", canonical.display()), &entries)
}

fn read_archive(bytes: &[u8]) -> Result<Vec<ContentEntry>, SourceError> {
    let count = validate_zip_directory(bytes)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(archive_error)?;
    if archive.len() != count {
        return Err(rejected("ambiguous duplicate archive paths"));
    }
    let mut entries = Vec::new();
    let mut total = 0u64;
    let mut paths = BTreeMap::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(archive_error)?;
        let entry = read_entry(&mut file, &mut total)?;
        if paths
            .insert(entry.relative_path.to_lowercase(), entry.kind)
            .is_some()
        {
            return Err(rejected("duplicate or case-conflicting archive paths"));
        }
        entries.push(entry);
    }
    for path in paths.keys() {
        let mut ancestor = path.as_str();
        while let Some((parent, _)) = ancestor.rsplit_once('/') {
            if paths.get(parent) == Some(&ContentEntryKind::File) {
                return Err(rejected(
                    "an archive file is also used as a parent directory",
                ));
            }
            ancestor = parent;
        }
    }
    Ok(entries)
}

fn read_entry<R: Read>(
    file: &mut zip::read::ZipFile<'_, R>,
    total: &mut u64,
) -> Result<ContentEntry, SourceError> {
    let raw =
        std::str::from_utf8(file.name_raw()).map_err(|_| rejected("entry path is not UTF-8"))?;
    let path = if file.is_dir() {
        raw.strip_suffix('/').unwrap_or(raw)
    } else {
        raw
    };
    if path.is_empty() {
        return Err(rejected("empty archive path"));
    }
    validate_subpath(path)?;
    let path = path.to_owned();
    let kind = if file.is_dir() {
        ContentEntryKind::Dir
    } else {
        ContentEntryKind::File
    };
    let mode = file.unix_mode().unwrap_or(0);
    let expected_type = if kind == ContentEntryKind::Dir {
        0o040000
    } else {
        0o100000
    };
    if file.encrypted()
        || file.is_symlink()
        || (mode & 0o170000 != 0 && mode & 0o170000 != expected_type)
    {
        return Err(rejected(
            "encrypted, linked or special entries are not supported",
        ));
    }
    let size = file.size();
    let limit = MAX_CONTENT_FILE_BYTES.min(MAX_SOURCE_BYTES.saturating_sub(*total));
    if size > limit || (kind == ContentEntryKind::Dir && size != 0) {
        return Err(rejected("entry or expanded archive exceeds the size limit"));
    }
    let mut bytes = Vec::new();
    file.take(size + 1)
        .read_to_end(&mut bytes)
        .map_err(archive_error)?;
    if bytes.len() as u64 != size {
        return Err(rejected(
            "entry is truncated or larger than its declared size",
        ));
    }
    *total += size;
    #[cfg(unix)]
    let permissions = if mode == 0 { 0o644 } else { mode & 0o777 };
    #[cfg(not(unix))]
    let permissions = 0o644;
    Ok(ContentEntry {
        relative_path: path,
        kind,
        bytes,
        mode: if kind == ContentEntryKind::Dir {
            0o755
        } else {
            permissions
        },
    })
}

fn archive_candidates(
    identity: String,
    entries: &[ContentEntry],
) -> Result<Vec<SkillCandidate>, SourceError> {
    let roots: Vec<_> = entries
        .iter()
        .filter(|entry| entry.kind == ContentEntryKind::File)
        .filter_map(|entry| {
            if entry.relative_path == "SKILL.md" {
                Some("")
            } else {
                entry.relative_path.strip_suffix("/SKILL.md")
            }
        })
        .collect();
    if roots.is_empty() {
        return Err(rejected("archive does not contain a SKILL.md file"));
    }
    if roots.len() > MAX_SOURCE_CANDIDATES {
        return Err(rejected("archive contains too many Skills"));
    }
    let mut candidates = Vec::new();
    for root in roots {
        let content = entries
            .iter()
            .filter_map(|entry| {
                relative_to(&entry.relative_path, root).map(|relative| ContentEntry {
                    relative_path: relative.into(),
                    ..entry.clone()
                })
            })
            .collect();
        candidates.push(finish_candidate(
            identity.clone(),
            root.into(),
            None,
            content,
        )?);
        check_candidate_totals(&candidates)?;
    }
    Ok(candidates)
}

fn rejected(message: &str) -> SourceError {
    SourceError::Rejected(format!("ZIP validation failed: {message}"))
}

fn unreachable(error: std::io::Error) -> SourceError {
    SourceError::Unreachable(format!("ZIP file could not be read: {error}"))
}

fn archive_error(error: impl std::fmt::Display) -> SourceError {
    SourceError::Rejected(format!("ZIP could not be read completely: {error}"))
}
