use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use asb_core::extensions::skill::ContentEntry;
use asb_core::extensions::validate::{
    ContentEntryKind, MAX_CONTENT_FILES, MAX_CONTENT_FILE_BYTES, MAX_CONTENT_TOTAL_BYTES,
};

use super::{
    check_candidate_totals, finish_candidate, validate_entries, SkillCandidate, SourceError,
    MAX_SOURCE_ENTRIES,
};

/// Accept either a Skill directory or a directory of Skills. Native discovery
/// also uses the latter form to revalidate an observed immediate child.
pub fn scan_local_source(
    root: &Path,
    resolved_commit: Option<String>,
) -> Result<Vec<SkillCandidate>, SourceError> {
    let root_metadata = metadata(root)?;
    if !root_metadata.is_dir() {
        return Err(SourceError::Rejected("来源必须是目录".into()));
    }
    let roots = if root.join("SKILL.md").is_file() {
        vec![root.to_path_buf()]
    } else {
        let mut roots = Vec::new();
        for child in list_directory(root)? {
            if metadata(&child)?.is_dir() && child.join("SKILL.md").is_file() {
                roots.push(child);
            }
        }
        roots
    };
    let mut candidates = Vec::new();
    for child in roots {
        let subpath = child
            .strip_prefix(root)
            .expect("source child")
            .to_string_lossy()
            .replace('\\', "/");
        let entries = walk_local(&child)?;
        let candidate = finish_candidate(
            root.to_string_lossy().into(),
            subpath,
            resolved_commit.clone(),
            entries,
        )
        .map_err(|error| SourceError::Rejected(format!("{}: {error}", child.display())))?;
        candidates.push(candidate);
        check_candidate_totals(&candidates)?;
    }
    Ok(candidates)
}

fn list_directory(root: &Path) -> Result<Vec<PathBuf>, SourceError> {
    let directory = fs::read_dir(root).map_err(|error| {
        SourceError::Unreachable(format!("无法读取 {}：{error}", root.display()))
    })?;
    let mut paths = Vec::new();
    for entry in directory {
        let entry = entry
            .map_err(|error| SourceError::Unreachable(format!("无法完整读取目录：{error}")))?;
        paths.push(entry.path());
        if paths.len() > MAX_SOURCE_ENTRIES {
            return Err(SourceError::Rejected(format!(
                "目录超过 {MAX_SOURCE_ENTRIES} 个条目"
            )));
        }
    }
    paths.sort();
    Ok(paths)
}

fn metadata(path: &Path) -> Result<fs::Metadata, SourceError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        SourceError::Unreachable(format!("无法读取 {}：{error}", path.display()))
    })?;
    #[cfg(windows)]
    let is_reparse = {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    };
    #[cfg(not(windows))]
    let is_reparse = false;
    if metadata.file_type().is_symlink() || is_reparse {
        return Err(SourceError::Rejected(format!(
            "{} 是链接或重解析点",
            path.display()
        )));
    }
    if !metadata.is_file() && !metadata.is_dir() {
        return Err(SourceError::Rejected(format!(
            "{} 不是普通文件或目录",
            path.display()
        )));
    }
    Ok(metadata)
}

fn walk_local(root: &Path) -> Result<Vec<ContentEntry>, SourceError> {
    let mut entries = Vec::new();
    let mut total = 0u64;
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((directory, prefix)) = stack.pop() {
        for path in list_directory(&directory)? {
            if entries.len() >= MAX_CONTENT_FILES {
                return Err(SourceError::Rejected(format!(
                    "Skill 超过 {MAX_CONTENT_FILES} 个条目"
                )));
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| SourceError::Rejected("文件名不是有效 UTF-8".into()))?;
            let relative = if prefix.is_empty() {
                name.into()
            } else {
                format!("{prefix}/{name}")
            };
            super::github::validate_subpath(&relative)?;
            let metadata = metadata(&path)?;
            let entry = if metadata.is_dir() {
                stack.push((path, relative.clone()));
                ContentEntry {
                    relative_path: relative,
                    kind: ContentEntryKind::Dir,
                    bytes: Vec::new(),
                    mode: 0o755,
                }
            } else {
                read_file(&path, relative, &metadata, &mut total)?
            };
            entries.push(entry);
        }
    }
    validate_entries(&entries)?;
    Ok(entries)
}

fn read_file(
    path: &Path,
    relative: String,
    metadata: &fs::Metadata,
    total: &mut u64,
) -> Result<ContentEntry, SourceError> {
    let limit = MAX_CONTENT_FILE_BYTES.min(MAX_CONTENT_TOTAL_BYTES.saturating_sub(*total));
    if metadata.len() > limit {
        return Err(SourceError::Rejected(format!(
            "{relative} 超过 Skill 内容大小上限"
        )));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(limit + 1).read_to_end(&mut bytes))
        .map_err(|error| SourceError::Unreachable(format!("无法读取 {relative}：{error}")))?;
    if bytes.len() as u64 > limit {
        return Err(SourceError::Rejected(format!(
            "{relative} 读取时超过内容大小上限"
        )));
    }
    *total += bytes.len() as u64;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o777
    };
    #[cfg(not(unix))]
    let mode = 0o644;
    Ok(ContentEntry {
        relative_path: relative,
        kind: ContentEntryKind::File,
        bytes,
        mode,
    })
}
