//! Walking local skill directories into canonical entries and scanning the
//! per-client skill roots.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{ExtensionKind, ObservedExtension, ObservedOrigin};
use asb_core::extensions::diagnostics::{DiagnosticCode, DiagnosticRemediation};
use asb_core::extensions::skill::{
    content_digest, extract_manifest, ContentEntry, ManifestExtraction,
};
use asb_core::extensions::validate::ContentEntryKind;

use super::{skill_location_label, DiagnosticSeed, DiagnosticSubjectSeed, DiscoveredPaths};

/// Why walking one skill directory failed, typed for the diagnostic it
/// becomes. The message is display text; the code is the contract.
#[derive(Debug, Clone)]
pub(crate) struct SkillWalkError {
    pub code: DiagnosticCode,
    pub message: String,
}

impl SkillWalkError {
    fn new(code: DiagnosticCode, message: &str) -> Self {
        Self {
            code,
            message: message.to_string(),
        }
    }
}

/// One observed skill directory plus the typed manifest problem it carries,
/// if any. A directory whose manifest is missing, frontmatter-less, or
/// unparsable still becomes an observation row: it exists on disk and the
/// user must see and fix it.
pub(crate) struct SkillObservation {
    pub observed: ObservedExtension,
    pub problem: Option<SkillProblem>,
}

/// The manifest problem of one observed skill directory.
pub(crate) struct SkillProblem {
    pub code: DiagnosticCode,
    pub message: String,
}

pub(crate) fn walk_skill_dir(root: &Path) -> Result<Vec<ContentEntry>, SkillWalkError> {
    let mut entries = Vec::new();
    let mut stack = vec![(root.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = stack.pop() {
        let directory_entries = fs::read_dir(&dir)
            .map_err(|_| SkillWalkError::new(DiagnosticCode::SkillDirUnreadable, "目录无法读取"))?;
        let mut children = Vec::new();
        for child in directory_entries {
            let child = child.map_err(|_| {
                SkillWalkError::new(DiagnosticCode::SkillDirUnreadable, "目录项无法读取")
            })?;
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
            let metadata = fs::symlink_metadata(&child).map_err(|_| {
                SkillWalkError::new(DiagnosticCode::SkillDirUnreadable, "目录项元数据无法读取")
            })?;
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                // Links are reported, never followed.
                return Err(SkillWalkError::new(
                    DiagnosticCode::SkillEntryLink,
                    "包含链接或重解析点，未读取其内容",
                ));
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
                let bytes = fs::read(&child).map_err(|_| {
                    SkillWalkError::new(DiagnosticCode::SkillDirUnreadable, "文件无法读取")
                })?;
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
                return Err(SkillWalkError::new(
                    DiagnosticCode::SkillEntryUnsupported,
                    "包含不支持的文件类型",
                ));
            }
        }
    }
    Ok(entries)
}

pub(crate) fn observed_skill(
    client: AppKind,
    origin: ObservedOrigin,
    dir: &Path,
) -> Result<SkillObservation, SkillWalkError> {
    let entries = walk_skill_dir(dir)?;
    let directory_name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let base_origin = origin.clone();
    let base = move |name: String, problem: Option<SkillProblem>| SkillObservation {
        observed: ObservedExtension {
            kind: ExtensionKind::Skill,
            client,
            name,
            description: None,
            origin: base_origin.clone(),
            path: dir.to_string_lossy().to_string(),
            managed_binding_ids: Vec::new(),
            content_digest: None,
            transport: None,
        },
        problem,
    };
    let Some(manifest_text) = entries
        .iter()
        .find(|entry| entry.relative_path == "SKILL.md")
        .map(|entry| String::from_utf8_lossy(&entry.bytes).to_string())
    else {
        // The file itself is missing; the directory name is never promoted
        // into a fabricated manifest.
        return Ok(base(
            directory_name,
            Some(SkillProblem {
                code: DiagnosticCode::SkillManifestMissing,
                message: "Skill 目录缺少 SKILL.md".to_string(),
            }),
        ));
    };
    match extract_manifest(&manifest_text) {
        ManifestExtraction::Parsed(manifest) => Ok(SkillObservation {
            observed: ObservedExtension {
                kind: ExtensionKind::Skill,
                client,
                name: manifest.name,
                description: manifest.description,
                origin,
                path: dir.to_string_lossy().to_string(),
                managed_binding_ids: Vec::new(),
                content_digest: Some(content_digest(&entries)),
                transport: None,
            },
            problem: None,
        }),
        ManifestExtraction::MissingManifest => Ok(base(
            directory_name,
            Some(SkillProblem {
                code: DiagnosticCode::SkillFrontmatterMissing,
                message: "SKILL.md 存在但缺少 frontmatter".to_string(),
            }),
        )),
        ManifestExtraction::Unreadable(failure) => Ok(base(
            directory_name,
            Some(SkillProblem {
                code: DiagnosticCode::SkillFrontmatterInvalid,
                message: format!("SKILL.md 的 frontmatter 无法解析：{}", failure.message),
            }),
        )),
    }
}

pub(super) fn scan_skills_root(
    client: AppKind,
    root: &Path,
    origin: impl Fn(&Path) -> ObservedOrigin,
    out: &mut Vec<ObservedExtension>,
    diagnostics: &mut Vec<DiagnosticSeed>,
) {
    let children = match fs::read_dir(root) {
        Ok(children) => children,
        Err(error) if error.kind() == ErrorKind::NotFound => return,
        Err(_) => {
            diagnostics.push(DiagnosticSeed {
                code: DiagnosticCode::SkillRootUnreadable,
                client,
                subject: DiagnosticSubjectSeed::Location {
                    label: "Skill 根目录".to_string(),
                    location_key: root.to_string_lossy().to_string(),
                    resource_kind: ExtensionKind::Skill,
                },
                message: "无法读取 Skill 根目录".to_string(),
                remediation: DiagnosticRemediation::Manual {
                    reason: "请检查目录权限后重新扫描".to_string(),
                },
            });
            return;
        }
    };
    let mut paths = Vec::new();
    for child in children {
        match child {
            Ok(child) => paths.push(child.path()),
            Err(_) => diagnostics.push(DiagnosticSeed {
                code: DiagnosticCode::SkillRootEntryUnreadable,
                client,
                subject: DiagnosticSubjectSeed::Location {
                    label: "Skill 根目录".to_string(),
                    location_key: root.to_string_lossy().to_string(),
                    resource_kind: ExtensionKind::Skill,
                },
                message: "无法读取 Skill 根目录中的一个目录项".to_string(),
                remediation: DiagnosticRemediation::Manual {
                    reason: "请检查目录项权限后重新扫描".to_string(),
                },
            }),
        }
    }
    paths.sort();
    for child in paths {
        let observed_origin = origin(&child);
        let metadata = match fs::symlink_metadata(&child) {
            Ok(metadata) => metadata,
            Err(_) => {
                diagnostics.push(DiagnosticSeed {
                    code: DiagnosticCode::SkillRootEntryUnreadable,
                    client,
                    subject: DiagnosticSubjectSeed::Location {
                        label: skill_location_label(&observed_origin, &child),
                        location_key: child.to_string_lossy().to_string(),
                        resource_kind: ExtensionKind::Skill,
                    },
                    message: "无法读取 Skill 目录元数据".to_string(),
                    remediation: DiagnosticRemediation::Manual {
                        reason: "请检查该目录的权限后重新扫描".to_string(),
                    },
                });
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            diagnostics.push(DiagnosticSeed {
                code: DiagnosticCode::SkillEntryLink,
                client,
                subject: DiagnosticSubjectSeed::Location {
                    label: skill_location_label(&observed_origin, &child),
                    location_key: child.to_string_lossy().to_string(),
                    resource_kind: ExtensionKind::Skill,
                },
                message: "发现链接或重解析点，未作为 Skill 读取".to_string(),
                remediation: DiagnosticRemediation::Manual {
                    reason: "本应用不会自动删除链接或替换目录；请人工确认其内容".to_string(),
                },
            });
            continue;
        }
        if metadata.is_dir() {
            match observed_skill(client, observed_origin.clone(), &child) {
                Ok(observation) => {
                    if let Some(problem) = observation.problem {
                        diagnostics.push(DiagnosticSeed {
                            code: problem.code,
                            client,
                            subject: DiagnosticSubjectSeed::Entry {
                                observation_index: out.len(),
                            },
                            message: problem.message,
                            remediation: DiagnosticRemediation::Manual {
                                reason: "缺少清单内容或必要字段时不能自动虚构；请补齐后重新扫描"
                                    .to_string(),
                            },
                        });
                    }
                    out.push(observation.observed);
                }
                Err(error) => diagnostics.push(DiagnosticSeed {
                    code: error.code,
                    client,
                    subject: DiagnosticSubjectSeed::Location {
                        label: skill_location_label(&observed_origin, &child),
                        location_key: child.to_string_lossy().to_string(),
                        resource_kind: ExtensionKind::Skill,
                    },
                    message: error.message,
                    remediation: DiagnosticRemediation::Manual {
                        reason: "请人工确认该目录内容后重新扫描".to_string(),
                    },
                }),
            }
        }
    }
}

pub(super) fn scan_codex_skills(
    paths: &DiscoveredPaths,
    out: &mut Vec<ObservedExtension>,
    diagnostics: &mut Vec<DiagnosticSeed>,
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
    diagnostics: &mut Vec<DiagnosticSeed>,
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
