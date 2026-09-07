//! Walking local skill directories into canonical entries and scanning the
//! per-client skill roots.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{ExtensionKind, ObservedExtension, ObservedOrigin};
use asb_core::extensions::skill::{
    content_digest, extract_manifest, ContentEntry, ManifestExtraction,
};
use asb_core::extensions::validate::ContentEntryKind;

use super::{DiscoveredPaths, DiscoveryDiagnostic};

pub(super) fn walk_skill_dir(root: &Path) -> Result<Vec<ContentEntry>, String> {
    let mut entries = Vec::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let directory_entries = fs::read_dir(&dir).map_err(|_| "目录无法读取".to_string())?;
        let mut children = Vec::new();
        for child in directory_entries {
            let child = child.map_err(|_| "目录项无法读取".to_string())?;
            children.push(child.path());
        }
        children.sort();
        for child in children {
            let name = child
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let metadata =
                fs::symlink_metadata(&child).map_err(|_| "目录项元数据无法读取".to_string())?;
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                // Links are reported, never followed.
                return Err("包含链接或重解析点，未读取其内容".to_string());
            }
            if file_type.is_dir() {
                entries.push(ContentEntry {
                    relative_path: relative.clone(),
                    kind: ContentEntryKind::Dir,
                    bytes: Vec::new(),
                    mode: 0o755,
                });
                stack.push((child, relative));
            } else if file_type.is_file() {
                let bytes = fs::read(&child).map_err(|_| "文件无法读取".to_string())?;
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
            } else {
                return Err("包含不支持的文件类型".to_string());
            }
        }
    }
    Ok(entries)
}

pub(super) fn observed_skill(
    client: AppKind,
    origin: ObservedOrigin,
    dir: &Path,
    diagnostics: Vec<String>,
) -> Result<ObservedExtension, String> {
    let entries = walk_skill_dir(dir)?;
    let manifest_text = entries
        .iter()
        .find(|entry| entry.relative_path == "SKILL.md")
        .map(|entry| String::from_utf8_lossy(&entry.bytes).to_string())
        .ok_or_else(|| "Skill 目录缺少 SKILL.md".to_string())?;
    let (name, description) = match extract_manifest(&manifest_text) {
        ManifestExtraction::Parsed(manifest) => (manifest.name, manifest.description),
        ManifestExtraction::MissingManifest => {
            return Ok(ObservedExtension {
                kind: ExtensionKind::Skill,
                client,
                name: dir
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default(),
                description: None,
                origin,
                path: dir.to_string_lossy().to_string(),
                managed_binding_ids: Vec::new(),
                content_digest: None,
                transport: None,
                diagnostics: vec!["目录缺少 SKILL.md，不构成有效 Skill".to_string()],
            });
        }
        ManifestExtraction::Unreadable(failure) => (
            dir.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default(),
            Some(failure.message),
        ),
    };
    Ok(ObservedExtension {
        kind: ExtensionKind::Skill,
        client,
        name,
        description,
        origin,
        path: dir.to_string_lossy().to_string(),
        managed_binding_ids: Vec::new(),
        content_digest: Some(content_digest(&entries)),
        transport: None,
        diagnostics,
    })
}

pub(super) fn scan_skills_root(
    client: AppKind,
    root: &Path,
    origin: impl Fn(&Path) -> ObservedOrigin,
    out: &mut Vec<ObservedExtension>,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) {
    let children = match fs::read_dir(root) {
        Ok(children) => children,
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(_) => {
            diagnostics.push(DiscoveryDiagnostic {
                client,
                path: root.to_string_lossy().to_string(),
                message: "无法读取 Skill 根目录".to_string(),
            });
            return;
        }
    };
    let mut paths = Vec::new();
    for child in children {
        match child {
            Ok(child) => paths.push(child.path()),
            Err(_) => diagnostics.push(DiscoveryDiagnostic {
                client,
                path: root.to_string_lossy().to_string(),
                message: "无法读取 Skill 根目录中的一个目录项".to_string(),
            }),
        }
    }
    paths.sort();
    for child in paths {
        let metadata = match fs::symlink_metadata(&child) {
            Ok(metadata) => metadata,
            Err(_) => {
                diagnostics.push(DiscoveryDiagnostic {
                    client,
                    path: child.to_string_lossy().to_string(),
                    message: "无法读取 Skill 目录元数据".to_string(),
                });
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            diagnostics.push(DiscoveryDiagnostic {
                client,
                path: child.to_string_lossy().to_string(),
                message: "发现链接或重解析点，未作为 Skill 读取".to_string(),
            });
            continue;
        }
        if metadata.is_dir() {
            match observed_skill(client, origin(&child), &child, Vec::new()) {
                Ok(observed) => out.push(observed),
                Err(message) => diagnostics.push(DiscoveryDiagnostic {
                    client,
                    path: child.to_string_lossy().to_string(),
                    message,
                }),
            }
        }
    }
}

pub(super) fn scan_codex_skills(
    paths: &DiscoveredPaths,
    out: &mut Vec<ObservedExtension>,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) {
    scan_skills_root(
        AppKind::Codex,
        &crate::extensions::paths::codex_user_skills_root(&paths.home),
        |_| ObservedOrigin::UserRoot,
        out,
        diagnostics,
    );
    for legacy in crate::extensions::paths::codex_legacy_skills_roots(
        &paths.home,
        paths.codex_home.as_deref(),
    ) {
        scan_skills_root(
            AppKind::Codex,
            &legacy,
            |path| ObservedOrigin::LegacyRoot {
                detail: format!("{}（历史目录，只读展示）", path.display()),
            },
            out,
            diagnostics,
        );
    }
}

pub(super) fn scan_claude_skills(
    paths: &DiscoveredPaths,
    out: &mut Vec<ObservedExtension>,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) {
    scan_skills_root(
        AppKind::Claude,
        &crate::extensions::paths::claude_user_skills_root(
            &paths.home,
            paths.claude_dir.as_deref(),
        ),
        |_| ObservedOrigin::UserRoot,
        out,
        diagnostics,
    );
}
