//! Read-only discovery of Skills and MCP servers already present on the
//! machine, plus the file-consistency status of managed bindings.
//!
//! Discovery never creates directories, links, indexes, or client
//! configuration. A fresh environment yields an empty result and no writes.

use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use self::mcp::{
    append_claude_project_private_mcp_observations, append_mcp_observations,
    read_discovery_document,
};
use self::skills::{scan_claude_skills, scan_codex_skills, scan_skills_root, walk_skill_dir};
use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    ExtensionBinding, ExtensionDefinition, ExtensionKind, ExtensionPayload, ExtensionTarget,
    FileState, ManagedBaseline, ManagedBaselineFile, ObservedExtension, ObservedOrigin,
};
use asb_core::extensions::diagnostics::{
    DiagnosticCode, DiagnosticRemediation, DiagnosticSubject, ExtensionDiagnostic,
};
use asb_core::extensions::skill::content_digest;

pub struct DiscoveredPaths {
    pub home: std::path::PathBuf,
    pub codex_home: Option<std::path::PathBuf>,
    pub claude_dir: Option<std::path::PathBuf>,
}

impl DiscoveredPaths {
    /// Reads the process environment exactly like `local_state`.
    pub fn from_env() -> Result<Self, String> {
        let home = crate::local_state::user_home_dir()?;
        let codex_home = std::env::var_os("CODEX_HOME")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from);
        let claude_dir = std::env::var_os("CLAUDE_CONFIG_DIR")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from);
        Ok(Self {
            home,
            codex_home,
            claude_dir,
        })
    }
}

/// Walks one skill directory into canonical entries. Callers surface every
/// read failure as a discovery diagnostic instead of silently dropping it.

pub struct DiscoveryResult {
    pub observed: Vec<ObservedExtension>,
    pub diagnostics: Vec<DiagnosticSeed>,
}

/// A problem found during one scan, before scan-wide identities exist.
/// Entry subjects reference the observation by its position in
/// [`DiscoveryResult::observed`]; the workspace resolves them to stable ids.
#[derive(Debug, Clone)]
pub struct DiagnosticSeed {
    pub code: DiagnosticCode,
    pub client: AppKind,
    pub subject: DiagnosticSubjectSeed,
    pub message: String,
    pub remediation: DiagnosticRemediation,
}

#[derive(Debug, Clone)]
pub enum DiagnosticSubjectSeed {
    /// Resolved to `DiagnosticSubject::DiscoveryEntry` with the observation's
    /// assigned id.
    Entry { observation_index: usize },
    /// Resolved to `DiagnosticSubject::ManagedBinding`.
    Binding { binding_id: String },
    /// Already final; the label names a renderer-safe scope, never a path,
    /// and the kind keeps the problem under its workspace tab.
    Location {
        label: String,
        /// Backend-only identity of a physical location. It prevents two
        /// same-labelled documents from collapsing without exposing a path.
        location_key: String,
        resource_kind: asb_core::extensions::contracts::ExtensionKind,
    },
}

/// Renders the renderer-safe location label of one skill-scope problem.
pub(super) fn skill_location_label(origin: &ObservedOrigin, dir: &Path) -> String {
    let name = dir
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();
    match origin {
        ObservedOrigin::ProjectRoot { .. } => format!("项目 Skill 目录 {name}"),
        ObservedOrigin::LegacyRoot { .. } => format!("历史 Skill 目录 {name}"),
        _ => format!("Skill 目录 {name}"),
    }
}

pub use managed::managed_binding_diagnostic;

pub fn discover(
    paths: &DiscoveredPaths,
    projects: &[asb_core::extensions::contracts::ProjectRegistration],
) -> DiscoveryResult {
    let mut observed = Vec::new();
    let mut diagnostics = Vec::new();

    scan_codex_skills(paths, &mut observed, &mut diagnostics);
    scan_claude_skills(paths, &mut observed, &mut diagnostics);

    for client in [AppKind::Codex, AppKind::Claude] {
        let document = crate::extensions::paths::user_mcp_document(
            client,
            &paths.home,
            paths.codex_home.as_deref(),
            paths.claude_dir.as_deref(),
        );
        if let Some(text) =
            read_discovery_document(client, "MCP 配置文档", &document, &mut diagnostics)
        {
            append_mcp_observations(
                client,
                &document,
                ObservedOrigin::UserRoot,
                &text,
                &mut observed,
                &mut diagnostics,
            );
            if client == AppKind::Claude {
                for project in projects {
                    append_claude_project_private_mcp_observations(
                        &document,
                        &project.root,
                        &text,
                        &mut observed,
                        &mut diagnostics,
                    );
                }
            }
        }
    }

    for project in projects {
        let root = std::path::PathBuf::from(&project.root);
        scan_skills_root(
            AppKind::Codex,
            &crate::extensions::paths::codex_project_skills_root(&root),
            |_| ObservedOrigin::ProjectRoot {
                project_path: project.root.clone(),
            },
            &mut observed,
            &mut diagnostics,
        );
        scan_skills_root(
            AppKind::Claude,
            &crate::extensions::paths::claude_project_skills_root(&root),
            |_| ObservedOrigin::ProjectRoot {
                project_path: project.root.clone(),
            },
            &mut observed,
            &mut diagnostics,
        );
        for (client, document) in [
            (
                AppKind::Codex,
                crate::extensions::paths::codex_project_config_path(&root),
            ),
            (
                AppKind::Claude,
                crate::extensions::paths::claude_project_mcp_path(&root),
            ),
        ] {
            if let Some(text) =
                read_discovery_document(client, "项目 MCP 配置文档", &document, &mut diagnostics)
            {
                append_mcp_observations(
                    client,
                    &document,
                    ObservedOrigin::ProjectRoot {
                        project_path: project.root.clone(),
                    },
                    &text,
                    &mut observed,
                    &mut diagnostics,
                );
            }
        }
    }

    DiscoveryResult {
        observed,
        diagnostics,
    }
}

/// Computes the file-consistency state of one binding against the current
/// disk and its baseline. `projects` resolves project-scoped targets.

pub fn binding_file_state(
    binding: &ExtensionBinding,
    definition: &ExtensionDefinition,
    baseline: Option<&ManagedBaselineFile>,
    projects: &[asb_core::extensions::contracts::ProjectRegistration],
) -> FileState {
    // Anything newer in the definition than the last apply is pending.
    let applied_current_revision = binding.last_applied_revision == Some(definition.revision);
    if !applied_current_revision && binding.last_applied_revision.is_none() {
        return FileState::NotDeployed;
    }
    match &definition.payload {
        ExtensionPayload::Mcp(_) => {
            let Some(ManagedBaseline::DocumentEntry {
                target_path,
                last_written_value,
                last_document_hash,
                ..
            }) = baseline.and_then(|file| {
                file.entries
                    .iter()
                    .find(|entry| matches!(entry, ManagedBaseline::DocumentEntry { .. }))
            })
            else {
                return FileState::NotDeployed;
            };
            let text = match fs::read_to_string(Path::new(target_path)) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => return FileState::Missing,
                Err(_) => return FileState::Unreadable,
            };
            // 所有权按条目判定（E02）：文档级哈希只作快路径。切换投影、
            // 片段合并等第一方写入会改文档其他部分，只要本应用的条目仍与
            // 最后写入一致，绑定就处于同步状态；条目本身变了才是外部变更。
            let document_unchanged = {
                use sha2::Digest;
                let current_hash = sha2::Sha256::digest(text.as_bytes())
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>();
                current_hash == *last_document_hash
            };
            if !document_unchanged {
                let current = asb_core::extensions::mcp::managed_entry_text(binding, &text, projects);
                let unchanged = match current {
                    Ok(current) => asb_core::extensions::mcp::managed_entry_unchanged(
                        binding.target.client(),
                        current,
                        last_written_value.as_deref(),
                    ),
                    Err(_) => false,
                };
                if !unchanged {
                    return FileState::ExternalChange;
                }
            }
            if applied_current_revision {
                FileState::InSync
            } else {
                FileState::PendingApply
            }
        }
        ExtensionPayload::Skill(_) => {
            let Some(ManagedBaseline::Directory {
                target_dir,
                last_digest,
                ..
            }) = baseline.and_then(|file| {
                file.entries
                    .iter()
                    .find(|entry| matches!(entry, ManagedBaseline::Directory { .. }))
            })
            else {
                return FileState::NotDeployed;
            };
            match walk_skill_dir(Path::new(target_dir)) {
                Ok(entries) => {
                    if content_digest(&entries) != *last_digest {
                        FileState::ExternalChange
                    // A pinned binding follows its locked content version, not
                    // the definition's current revision: editing the library
                    // must not make a pinned target look pending.
                    } else if match binding.locked_digest.as_deref() {
                        Some(locked) => last_digest == locked,
                        None => applied_current_revision,
                    } {
                        FileState::InSync
                    } else {
                        FileState::PendingApply
                    }
                }
                Err(_) if !Path::new(target_dir).exists() => FileState::Missing,
                Err(_) => FileState::Unreadable,
            }
        }
    }
}

/// Marks which observed items are managed by which bindings.
pub fn attach_bindings(
    mut observed: Vec<ObservedExtension>,
    bindings: &[ExtensionBinding],
    paths: &DiscoveredPaths,
    projects: &[asb_core::extensions::contracts::ProjectRegistration],
) -> Vec<ObservedExtension> {
    for observed_item in &mut observed {
        for binding in bindings {
            if binding_owns_observation(observed_item, binding, paths, projects) {
                observed_item.managed_binding_ids.push(binding.id.clone());
            }
        }
    }
    observed
}

/// A discovery row is managed only when its exact physical target belongs to
/// the binding. Matching a native key or directory suffix alone would mark
/// same-named entries in another client or project as owned.
fn binding_owns_observation(
    observed: &ObservedExtension,
    binding: &ExtensionBinding,
    paths: &DiscoveredPaths,
    projects: &[asb_core::extensions::contracts::ProjectRegistration],
) -> bool {
    if observed.client != binding.target.client() {
        return false;
    }
    match observed.kind {
        ExtensionKind::Skill => {
            let Some(name) = binding.deploy_name.as_deref() else {
                return false;
            };
            skill_binding_path(binding, name, paths, projects)
                .is_some_and(|path| path == Path::new(&observed.path))
        }
        ExtensionKind::Mcp => {
            if binding.native_key.as_deref() != Some(observed.name.as_str()) {
                return false;
            }
            if matches!(
                binding.target,
                ExtensionTarget::ProjectPrivate {
                    client: AppKind::Claude,
                    ..
                }
            ) {
                let Some(root) = project_root(binding, projects) else {
                    return false;
                };
                return matches!(
                    &observed.origin,
                    ObservedOrigin::ProjectRoot { project_path }
                        if project_path == &root.to_string_lossy()
                ) && crate::extensions::paths::claude_user_json_path(
                    &paths.home,
                    paths.claude_dir.as_deref(),
                ) == Path::new(&observed.path);
            }
            mcp_binding_document(binding, paths, projects)
                .is_some_and(|path| path == Path::new(&observed.path))
        }
    }
}

fn project_root<'a>(
    binding: &ExtensionBinding,
    projects: &'a [asb_core::extensions::contracts::ProjectRegistration],
) -> Option<&'a Path> {
    binding
        .target
        .project_id()
        .and_then(|id| projects.iter().find(|project| project.id == id))
        .map(|project| Path::new(&project.root))
}

fn skill_binding_path(
    binding: &ExtensionBinding,
    name: &str,
    paths: &DiscoveredPaths,
    projects: &[asb_core::extensions::contracts::ProjectRegistration],
) -> Option<std::path::PathBuf> {
    match &binding.target {
        ExtensionTarget::App {
            client: AppKind::Codex,
        } => Some(crate::extensions::paths::codex_user_skills_root(&paths.home).join(name)),
        ExtensionTarget::App {
            client: AppKind::Claude,
        } => Some(
            crate::extensions::paths::claude_user_skills_root(
                &paths.home,
                paths.claude_dir.as_deref(),
            )
            .join(name),
        ),
        ExtensionTarget::ProjectShared {
            client: AppKind::Codex,
            ..
        }
        | ExtensionTarget::ProjectPrivate {
            client: AppKind::Codex,
            ..
        } => project_root(binding, projects)
            .map(crate::extensions::paths::codex_project_skills_root)
            .map(|root| root.join(name)),
        ExtensionTarget::ProjectShared {
            client: AppKind::Claude,
            ..
        }
        | ExtensionTarget::ProjectPrivate {
            client: AppKind::Claude,
            ..
        } => project_root(binding, projects)
            .map(crate::extensions::paths::claude_project_skills_root)
            .map(|root| root.join(name)),
    }
}

fn mcp_binding_document(
    binding: &ExtensionBinding,
    paths: &DiscoveredPaths,
    projects: &[asb_core::extensions::contracts::ProjectRegistration],
) -> Option<std::path::PathBuf> {
    match &binding.target {
        ExtensionTarget::App { client } => Some(crate::extensions::paths::user_mcp_document(
            *client,
            &paths.home,
            paths.codex_home.as_deref(),
            paths.claude_dir.as_deref(),
        )),
        ExtensionTarget::ProjectShared {
            client: AppKind::Codex,
            ..
        } => {
            project_root(binding, projects).map(crate::extensions::paths::codex_project_config_path)
        }
        ExtensionTarget::ProjectShared {
            client: AppKind::Claude,
            ..
        } => project_root(binding, projects).map(crate::extensions::paths::claude_project_mcp_path),
        // Discovery currently reads top-level mcpServers only. A private
        // project entry lives under projects.<root>.mcpServers in the same
        // user document and must not be mistaken for a top-level entry.
        ExtensionTarget::ProjectPrivate { .. } => None,
    }
}

/// Turns one scan's seeds into final diagnostics: stable ids, resolved
/// subjects, and one diagnostic per (object, problem type) — objects are
/// never merged by message text. `new_id` mints the scan-scoped diagnostic
/// identities.
pub fn finalize_diagnostics(
    seeds: Vec<DiagnosticSeed>,
    observation_ids: &[String],
    new_id: &mut dyn FnMut(&str) -> String,
) -> Vec<ExtensionDiagnostic> {
    let mut out: Vec<ExtensionDiagnostic> = Vec::new();
    let mut seen: std::collections::BTreeSet<(AppKind, String, DiagnosticCode)> =
        std::collections::BTreeSet::new();
    for seed in seeds {
        let (subject, subject_key) = match seed.subject {
            DiagnosticSubjectSeed::Entry { observation_index } => {
                let Some(observation_id) = observation_ids.get(observation_index) else {
                    continue;
                };
                (
                    DiagnosticSubject::DiscoveryEntry {
                        observation_id: observation_id.clone(),
                    },
                    format!("entry:{observation_id}"),
                )
            }
            DiagnosticSubjectSeed::Binding { binding_id } => (
                DiagnosticSubject::ManagedBinding {
                    binding_id: binding_id.clone(),
                },
                format!("binding:{binding_id}"),
            ),
            DiagnosticSubjectSeed::Location {
                label,
                location_key,
                resource_kind,
            } => (
                DiagnosticSubject::ScanLocation {
                    label,
                    resource_kind,
                },
                format!("location:{location_key}"),
            ),
        };
        if !seen.insert((seed.client, subject_key, seed.code)) {
            continue;
        }
        out.push(ExtensionDiagnostic {
            id: new_id("diag"),
            code: seed.code,
            client: seed.client,
            subject,
            message: seed.message,
            remediation: seed.remediation,
        });
    }
    out
}

mod managed;
mod mcp;
mod skills;
#[cfg(test)]
mod tests;
