//! Skill sources: local directories and public GitHub repositories.
//!
//! Remote access happens only when the user requests it. A resolved source
//! produces candidates: every directory containing a `SKILL.md`, with its
//! provenance (repo identity + subpath), resolved commit, and a content
//! digest over the whole subtree. Nothing is installed or written to any
//! client by this module.

use std::io::Read;
use std::path::Path;

use asb_core::extensions::skill::{
    content_digest, extract_manifest, ContentEntry, ManifestExtraction,
};
use asb_core::extensions::validate::{
    validate_content_paths, ContentEntryKind, MAX_CONTENT_FILES, MAX_CONTENT_FILE_BYTES,
    MAX_CONTENT_TOTAL_BYTES,
};

/// One installable skill candidate from a source scan.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillCandidate {
    /// Canonical source identity: `<repo>` or the local root, plus subpath.
    pub source_identity: String,
    /// Path inside the source, `/`-separated (empty for the root itself).
    pub subpath: String,
    pub name: String,
    pub description: Option<String>,
    /// Resolved commit for GitHub sources.
    pub resolved_commit: Option<String>,
    pub content_digest: String,
    pub entries: Vec<ContentEntry>,
    pub diagnostics: Vec<String>,
}

/// Errors from source resolution.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourceError {
    #[error("来源不可达：{0}")]
    Unreachable(String),
    #[error("来源内容被拒绝：{0}")]
    Rejected(String),
}

/// The HTTP boundary. Tests inject a fake transport; production uses the
/// shared `probe::http_request`.
pub type HttpFetch<'a> = &'a dyn Fn(&str) -> Result<(u16, Vec<u8>), String>;

/// Resolves a git ref to a commit through the GitHub API.
pub fn resolve_github_commit(
    repo: &str,
    ref_name: &str,
    fetch: HttpFetch,
) -> Result<String, SourceError> {
    let url = format!("https://api.github.com/repos/{repo}/commits/{ref_name}");
    let (status, body) = fetch(&url).map_err(SourceError::Unreachable)?;
    if status != 200 {
        return Err(SourceError::Unreachable(format!(
            "GitHub 返回 HTTP {status}"
        )));
    }
    let value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|error| SourceError::Unreachable(error.to_string()))?;
    value
        .get("sha")
        .and_then(|sha| sha.as_str())
        .map(str::to_string)
        .ok_or_else(|| SourceError::Unreachable("GitHub 响应缺少 sha".to_string()))
}

/// The default fetcher built on the shared outbound HTTP client.
pub fn http_fetch(url: &str) -> Result<(u16, Vec<u8>), String> {
    crate::probe::http_bytes(url)
}

/// Downloads and extracts one skill subtree from a repository tarball.
pub fn fetch_github_subtree(
    repo: &str,
    commit: &str,
    subpath: &str,
    fetch: HttpFetch,
) -> Result<SkillCandidate, SourceError> {
    let url = format!("https://codeload.github.com/{repo}/tar.gz/{commit}");
    let (status, body) = fetch(&url).map_err(SourceError::Unreachable)?;
    if status != 200 {
        return Err(SourceError::Unreachable(format!(
            "tarball 下载返回 HTTP {status}"
        )));
    }
    let entries = extract_tar_gz_rooted(&body, subpath)?;
    finish_candidate(
        format!("{repo}"),
        subpath.to_string(),
        Some(commit.to_string()),
        entries,
    )
}

/// Scans a local directory one level for skill subdirectories.
pub fn scan_local_source(
    root: &Path,
    resolved_commit: Option<String>,
) -> Result<Vec<SkillCandidate>, SourceError> {
    let mut candidates = Vec::new();
    let entries = fs_err_list(root)?;
    let mut children: Vec<_> = entries;
    children.sort();
    for child in children {
        if !child.is_dir() {
            continue;
        }
        if !child.join("SKILL.md").is_file() {
            continue;
        }
        if let Ok(walked) = walk_local(&child) {
            if let Ok(candidate) = finish_candidate(
                root.to_string_lossy().to_string(),
                String::new(),
                resolved_commit.clone(),
                walked,
            ) {
                candidates.push(candidate);
            }
        }
    }
    Ok(candidates)
}

/// Extracts a tar.gz payload into entries under `subpath`. The archive
/// prefix (GitHub tarballs include `<repo>-<sha>/`) is stripped, and
/// path traversal, links, and oversize content abort the extraction before
/// anything is returned.
pub fn extract_tar_gz_rooted(
    payload: &[u8],
    subpath: &str,
) -> Result<Vec<ContentEntry>, SourceError> {
    let decoder = flate2::read::GzDecoder::new(payload);
    let mut archive = tar::Archive::new(decoder);
    archive.set_preserve_permissions(true);
    let wanted_prefix = if subpath.is_empty() {
        None
    } else {
        Some(format!("{}/", subpath.trim_matches('/')))
    };
    // The GitHub tarball root is `<repo>-<ref>/`; detect it from the first
    // entry and strip it so subpath matching is stable.
    let mut root_prefix: Option<String> = None;
    let mut entries: Vec<ContentEntry> = Vec::new();
    let mut total_bytes = 0u64;
    for entry in archive
        .entries()
        .map_err(|error| SourceError::Rejected(format!("压缩包无法读取：{error}")))?
    {
        let mut entry =
            entry.map_err(|error| SourceError::Rejected(format!("压缩包条目无法读取：{error}")))?;
        let raw_path = entry
            .path()
            .map_err(|error| SourceError::Rejected(error.to_string()))?
            .to_string_lossy()
            .to_string();
        if raw_path.is_empty() || raw_path.ends_with('/') && !raw_path.contains('/') {
            continue;
        }
        if root_prefix.is_none() {
            let first = raw_path.split('/').next().unwrap_or_default().to_string();
            if !first.is_empty() {
                root_prefix = Some(format!("{first}/"));
            }
        }
        let prefix_len = root_prefix.as_deref().map(str::len).unwrap_or(0);
        if raw_path.len() <= prefix_len {
            continue;
        }
        let relative = &raw_path[prefix_len..];
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(SourceError::Rejected(format!(
                "{relative} 是链接或特殊条目；已拒绝"
            )));
        }
        let relative = if let Some(wanted) = &wanted_prefix {
            match relative.strip_prefix(wanted) {
                Some(rest) if !rest.is_empty() => rest.to_string(),
                _ => continue,
            }
        } else {
            relative.to_string()
        };
        if entries.len() >= MAX_CONTENT_FILES {
            return Err(SourceError::Rejected(format!(
                "条目数超过上限 {MAX_CONTENT_FILES}"
            )));
        }
        #[cfg(unix)]
        let mode = {
            use std::os::unix::fs::PermissionsExt;
            entry.header().mode().unwrap_or(0o644)
        };
        #[cfg(not(unix))]
        let mode = 0o644;
        if entry_type.is_dir() {
            entries.push(ContentEntry {
                relative_path: relative,
                kind: ContentEntryKind::Dir,
                bytes: Vec::new(),
                mode: 0o755,
            });
            continue;
        }
        let size = entry.size();
        if size > MAX_CONTENT_FILE_BYTES {
            return Err(SourceError::Rejected(format!(
                "{relative} 超过单文件大小上限"
            )));
        }
        total_bytes += size;
        if total_bytes > MAX_CONTENT_TOTAL_BYTES {
            return Err(SourceError::Rejected(format!(
                "内容总大小超过上限 {MAX_CONTENT_TOTAL_BYTES} 字节"
            )));
        }
        let mut bytes = Vec::with_capacity(size as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| SourceError::Rejected(error.to_string()))?;
        entries.push(ContentEntry {
            relative_path: relative,
            kind: ContentEntryKind::File,
            bytes,
            mode,
        });
    }
    // Path safety before anything consumes the entries.
    let path_check: Vec<(String, ContentEntryKind, u64)> = entries
        .iter()
        .map(|entry| {
            (
                entry.relative_path.clone(),
                entry.kind,
                entry.bytes.len() as u64,
            )
        })
        .collect();
    validate_content_paths(&path_check)
        .map_err(|errors| SourceError::Rejected(errors.join("；")))?;
    Ok(entries)
}

fn finish_candidate(
    source_identity: String,
    subpath: String,
    resolved_commit: Option<String>,
    entries: Vec<ContentEntry>,
) -> Result<SkillCandidate, SourceError> {
    let manifest_text = entries
        .iter()
        .find(|entry| entry.relative_path == "SKILL.md")
        .map(|entry| String::from_utf8_lossy(&entry.bytes).to_string())
        .ok_or_else(|| SourceError::Rejected("子目录缺少 SKILL.md".to_string()))?;
    let (name, description, mut diagnostics) = match extract_manifest(&manifest_text) {
        ManifestExtraction::Parsed(manifest) => (manifest.name, manifest.description, Vec::new()),
        ManifestExtraction::Unreadable(failure) => (String::new(), None, vec![failure.message]),
        ManifestExtraction::MissingManifest => {
            return Err(SourceError::Rejected("子目录缺少 SKILL.md".to_string()));
        }
    };
    if !manifest_text.contains("---") {
        diagnostics.push("SKILL.md 没有 frontmatter".to_string());
    }
    Ok(SkillCandidate {
        source_identity,
        subpath,
        name,
        description,
        resolved_commit,
        content_digest: content_digest(&entries),
        entries,
        diagnostics,
    })
}

fn fs_err_list(root: &Path) -> Result<Vec<std::path::PathBuf>, SourceError> {
    std::fs::read_dir(root)
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .map_err(|error| SourceError::Unreachable(format!("无法读取目录：{error}")))
}

fn walk_local(root: &Path) -> Result<Vec<ContentEntry>, SourceError> {
    let mut entries = Vec::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        for child in fs_err_list(&dir)? {
            let name = child
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let metadata = std::fs::symlink_metadata(&child)
                .map_err(|error| SourceError::Rejected(error.to_string()))?;
            if metadata.file_type().is_symlink() {
                return Err(SourceError::Rejected(format!(
                    "{relative} 是链接；只接受可完整物化的普通目录"
                )));
            }
            if metadata.is_dir() {
                entries.push(ContentEntry {
                    relative_path: relative.clone(),
                    kind: ContentEntryKind::Dir,
                    bytes: Vec::new(),
                    mode: 0o755,
                });
                stack.push((child, relative));
            } else if metadata.is_file() {
                let bytes = std::fs::read(&child)
                    .map_err(|error| SourceError::Rejected(error.to_string()))?;
                #[cfg(unix)]
                let mode = {
                    use std::os::unix::fs::PermissionsExt;
                    metadata.permissions().mode() & 0o7777
                };
                #[cfg(not(unix))]
                let mode = 0o644;
                entries.push(ContentEntry {
                    relative_path: relative,
                    kind: ContentEntryKind::File,
                    bytes,
                    mode,
                });
            }
        }
    }
    let path_check: Vec<(String, ContentEntryKind, u64)> = entries
        .iter()
        .map(|entry| {
            (
                entry.relative_path.clone(),
                entry.kind,
                entry.bytes.len() as u64,
            )
        })
        .collect();
    validate_content_paths(&path_check)
        .map_err(|errors| SourceError::Rejected(errors.join("；")))?;
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_tar_gz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar_buffer = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buffer);
            for (path, bytes) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                builder.append_data(&mut header, path, *bytes).unwrap();
            }
            builder.finish().unwrap();
        }
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&tar_buffer).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn tarball_extraction_strips_the_repo_root_and_honors_subpath() {
        let payload = build_tar_gz(&[
            (
                "repo-abc123/skills/pdf/SKILL.md",
                b"---\nname: pdf\ndescription: PDF\n---\n",
            ),
            ("repo-abc123/skills/pdf/ref.md", b"reference"),
            ("repo-abc123/other/file.txt", b"ignored"),
        ]);
        let entries = extract_tar_gz_rooted(&payload, "skills/pdf").unwrap();
        let paths: Vec<&str> = entries
            .iter()
            .map(|entry| entry.relative_path.as_str())
            .collect();
        assert!(paths.contains(&"SKILL.md"));
        assert!(paths.contains(&"ref.md"));
        assert!(!paths.iter().any(|path| path.contains("other")));

        let candidate = finish_candidate(
            "org/repo".to_string(),
            "skills/pdf".to_string(),
            Some("abc123".to_string()),
            entries,
        )
        .unwrap();
        assert_eq!(candidate.name, "pdf");
        assert_eq!(candidate.resolved_commit.as_deref(), Some("abc123"));
    }

    #[test]
    fn traversal_entries_are_refused_before_anything_consumes_them() {
        // tar::Builder itself refuses `..` paths, so the fixture writes the
        // raw ustar blocks a crafted archive would carry.
        let mut tar_buffer = Vec::new();
        let name = b"repo-x/../../escape.md";
        let contents = b"nope";
        let mut block = [0u8; 512];
        block[..name.len()].copy_from_slice(name);
        block[100..108].copy_from_slice(b"0000644\0");
        block[108..116].copy_from_slice(b"0000000\0");
        block[116..124].copy_from_slice(b"0000000\0");
        let size_field = format!("{:06o}\0 ", contents.len());
        block[124..124 + size_field.len()].copy_from_slice(size_field.as_bytes());
        block[136..148].copy_from_slice(b"00000000000\0");
        block[156] = b'0';
        block[257..263].copy_from_slice(b"ustar\0");
        block[263..265].copy_from_slice(b"00");
        let mut sum = 0u32;
        for &byte in &block {
            sum += byte as u32;
        }
        let checksum = format!("{:06o}\0 ", sum);
        block[148..156].copy_from_slice(checksum.as_bytes());
        tar_buffer.extend_from_slice(&block);
        tar_buffer.extend_from_slice(contents);
        let padding = (512 - contents.len() % 512) % 512;
        tar_buffer.extend(std::iter::repeat(0u8).take(padding));
        tar_buffer.extend(std::iter::repeat(0u8).take(1024));
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&tar_buffer).unwrap();
        let payload = encoder.finish().unwrap();
        let error = extract_tar_gz_rooted(&payload, "").unwrap_err();
        assert!(matches!(error, SourceError::Rejected(_)));
    }

    #[test]
    fn link_entries_are_refused() {
        let mut tar_buffer = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buffer);
            let mut header = tar::Header::new_gnu();
            header.set_size(0);
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_mode(0o777);
            builder
                .append_data(&mut header, "repo-x/link", std::io::empty())
                .unwrap();
            builder.finish().unwrap();
        }
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(&tar_buffer).unwrap();
        let payload = encoder.finish().unwrap();
        let error = extract_tar_gz_rooted(&payload, "").unwrap_err();
        assert!(matches!(error, SourceError::Rejected(_)));
    }

    #[test]
    fn commit_resolution_reads_the_api_shape() {
        let fetch = |_url: &str| {
            Ok((
                200,
                br#"{"sha":"abc123","commit":{"message":"x"}}"#.to_vec(),
            ))
        };
        assert_eq!(
            resolve_github_commit("org/repo", "main", &fetch).unwrap(),
            "abc123"
        );
        let failing = |_url: &str| Ok((404, Vec::new()));
        assert!(resolve_github_commit("org/repo", "main", &failing).is_err());
    }
}
