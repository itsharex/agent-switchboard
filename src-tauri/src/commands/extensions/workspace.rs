//! The workspace read side: listing, interrupted-transaction recovery,
//! and native discovery with its renderer-safe observation views.

use std::fs;

use crate::extensions::store::LibraryCommit;
use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    ClientCapabilityReport, DependencyState, ExtensionKind, ExtensionOperationRecord,
    ExtensionPayload, ObservedExtension, ObservedOrigin, ProjectRegistration,
};
use asb_core::extensions::diagnostics::ExtensionDiagnostic;
use asb_core::extensions::CapabilityEnvironment;
use serde::Serialize;
use tauri::AppHandle;

use super::support::*;
use crate::commands::error::{blocking, state, CommandError};
use crate::extensions::discovery::{self, DiscoveredPaths};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionsWorkspaceDto {
    generation: u64,
    items: Vec<ExtensionListItemDto>,
    projects: Vec<ProjectRegistration>,
    history: Vec<ExtensionOperationRecord>,
    capabilities: Vec<ClientCapabilityReport>,
    /// Journals from interrupted operations whose recovery failed; the UI
    /// blocks further writes until the user resolves them.
    recovery_required: Vec<String>,
}

pub(super) fn capability_reports() -> Vec<ClientCapabilityReport> {
    let claude_custom = std::env::var_os("CLAUDE_CONFIG_DIR")
        .filter(|value| !value.is_empty())
        .is_some();
    let environment = CapabilityEnvironment {
        claude_config_dir_custom: claude_custom,
    };
    vec![
        asb_core::extensions::capability_report(AppKind::Codex, &environment),
        asb_core::extensions::capability_report(AppKind::Claude, &environment),
    ]
}

#[tauri::command]
pub async fn list_extensions(app: AppHandle) -> Result<ExtensionsWorkspaceDto, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let definitions = store.list_definitions().map_err(store_error)?;
        let bindings = store.list_bindings().map_err(store_error)?;
        let projects = store.list_projects().map_err(store_error)?;
        let checks = store.list_checks().map_err(store_error)?;
        let mut items = Vec::new();
        for definition in definitions {
            let mut binding_rows = Vec::new();
            for binding in bindings
                .iter()
                .filter(|row| row.resource_id == definition.id)
            {
                let baseline = store.get_baseline_file(&binding.id).map_err(store_error)?;
                let file_state = discovery::binding_file_state(
                    binding,
                    &definition,
                    baseline.as_ref(),
                    &projects,
                );
                binding_rows.push(BindingStatusDto {
                    binding: binding.clone(),
                    file_state,
                    warnings: Vec::new(),
                });
            }
            let dependency_states = match &definition.payload {
                ExtensionPayload::Skill(skill) => skill
                    .dependencies
                    .iter()
                    .map(|dependency| DependencyStatusDto {
                        state: match &dependency.resource_id {
                            None => DependencyState::PendingConfiguration,
                            Some(resource_id) => {
                                let linked = store.get_definition(resource_id).ok().flatten();
                                match linked {
                                    Some(linked) => {
                                        let supports = match linked.kind() {
                                            ExtensionKind::Mcp => bindings
                                                .iter()
                                                .any(|row| row.resource_id == resource_id.as_str()),
                                            ExtensionKind::Skill => false,
                                        };
                                        if supports {
                                            DependencyState::Bound
                                        } else {
                                            DependencyState::TargetUnsupported
                                        }
                                    }
                                    None => DependencyState::PendingConfiguration,
                                }
                            }
                        },
                        name: dependency.name.clone(),
                        resource_id: dependency.resource_id.clone(),
                    })
                    .collect(),
                ExtensionPayload::Mcp(_) => Vec::new(),
            };
            let last_check = checks
                .iter()
                .filter(|check| check.definition_id == definition.id)
                .max_by(|a, b| a.checked_at.cmp(&b.checked_at))
                .cloned();
            items.push(extension_list_item(
                definition,
                binding_rows,
                dependency_states,
                last_check,
            ));
        }
        // Listing stays read-only: interrupted transactions are reported
        // for the dedicated recovery command; recovery itself takes the
        // same target locks as the executor and never runs inline here.
        let recovery_required = store
            .pending_transactions()
            .into_iter()
            .map(|journal_dir| {
                journal_dir
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .filter(|id| !id.is_empty())
            .collect();
        Ok(ExtensionsWorkspaceDto {
            generation: store.manifest().map_err(store_error)?.generation,
            recovery_required,
            items,
            projects,
            history: store
                .list_history()
                .map_err(store_error)?
                .into_iter()
                .map(|snapshot| snapshot.record)
                .collect(),
            capabilities: capability_reports(),
        })
    })
    .await
}

/// Finishes interrupted transactions: for every pending journal, it takes
/// the same target locks the executor would, rolls the client files back,
/// and restores a half-applied library batch from the journaled pre-state.
#[tauri::command]
pub async fn recover_extension_transactions(app: AppHandle) -> Result<Vec<String>, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let active: Vec<String> = pending_plans()
            .lock()
            .expect("plans")
            .values()
            .map(|pending| pending.operation_id.clone())
            .collect();
        let mut results = Vec::new();
        for journal_dir in store.pending_transactions() {
            let operation_id = journal_dir
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            if active.contains(&operation_id) {
                results.push(format!("{operation_id}：仍在执行中，已跳过"));
                continue;
            }
            // A durable library receipt is written after the complete
            // library batch. It proves that client writes and library state
            // already committed even if the executor crashed before its
            // final journal line; cleanup must never undo that operation.
            let marker_exists = match store.has_commit_marker(&operation_id) {
                Ok(true) => {
                    match asb_switch::extensions::complete_recovery(&asb_switch::FsIo, &journal_dir)
                    {
                        Ok(()) => results.push(format!(
                            "{operation_id}：已确认持久化提交，已清理未完成事务标记"
                        )),
                        Err(error) => results.push(format!(
                            "{operation_id}：已确认持久化提交，但无法清理事务标记：{error}"
                        )),
                    }
                    true
                }
                Ok(false) => false,
                Err(error) => {
                    results.push(format!("{operation_id}：提交凭据无法读取：{error}"));
                    continue;
                }
            };
            if marker_exists {
                continue;
            }
            // Mutual exclusion with any concurrent writer: take every
            // target lock the journaled plan names, in the executor's
            // sorted order.
            let targets =
                asb_switch::extensions::journal_lock_targets(&asb_switch::FsIo, &journal_dir);
            let mut held = Vec::new();
            let mut blocked = false;
            for target in &targets {
                match asb_switch::lockfile::acquire(&asb_switch::FsIo, target, "agent-switchboard")
                {
                    asb_switch::lockfile::AcquireOutcome::Acquired => held.push(target.clone()),
                    asb_switch::lockfile::AcquireOutcome::Busy(_) => {
                        blocked = true;
                        break;
                    }
                }
            }
            if blocked {
                for lock in held.iter().rev() {
                    let _ = asb_switch::lockfile::release(&asb_switch::FsIo, lock);
                }
                results.push(format!("{operation_id}：目标被占用，暂不能恢复"));
                continue;
            }
            let Some(report) = asb_switch::recover_pending(&asb_switch::FsIo, &journal_dir) else {
                for lock in held.iter().rev() {
                    let _ = asb_switch::lockfile::release(&asb_switch::FsIo, lock);
                }
                continue;
            };
            if !report.failed.is_empty() {
                // A library restore after a client-file failure would create
                // a mixed operation state. Keep the full journal intact so
                // the same explicit recovery can retry both layers safely.
                results.push(format!(
                    "{operation_id}：客户端恢复未完成，事务已保留：{}",
                    report.failed.join("；")
                ));
            } else if let Some(library_recovery) = &report.library_recovery {
                match serde_json::from_value::<LibraryCommit>(library_recovery.clone()) {
                    Ok(commit) => match store.recover_library(&commit) {
                        Ok(()) => match asb_switch::extensions::complete_recovery(
                            &asb_switch::FsIo,
                            &journal_dir,
                        ) {
                            Ok(()) => results.push(format!(
                                "{operation_id}：客户端与扩展库均已恢复到操作前状态"
                            )),
                            Err(error) => results.push(format!(
                                "{operation_id}：已恢复状态，但无法清理事务标记：{error}"
                            )),
                        },
                        Err(error) => results.push(format!(
                            "{operation_id}：客户端已恢复，扩展库恢复失败：{error}"
                        )),
                    },
                    Err(error) => results.push(format!(
                        "{operation_id}：事务中的扩展库恢复数据无效：{error}"
                    )),
                }
            } else if report.completed_cleanly {
                results.push(format!("{operation_id}：已确认事务完成并清理标记"));
            } else {
                results.push(format!(
                    "{operation_id}：已恢复 {} 个客户端状态",
                    report.restored.len()
                ));
            }
            for lock in held.iter().rev() {
                let _ = asb_switch::lockfile::release(&asb_switch::FsIo, lock);
            }
        }
        Ok(results)
    })
    .await
}

#[tauri::command]
pub async fn discover_extensions(app: AppHandle) -> Result<ExtensionDiscoveryDto, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let projects = store.list_projects().map_err(store_error)?;
        let paths = DiscoveredPaths::from_env()
            .map_err(|error| CommandError::new("app-state-unavailable", error))?;
        let mut result = discovery::discover(&paths, &projects);
        let bindings = store.list_bindings().map_err(store_error)?;
        // Managed-binding consistency joins the same diagnostic collection:
        // only scanning surviving directories would miss missing objects.
        for binding in &bindings {
            let Some(definition) = store
                .get_definition(&binding.resource_id)
                .map_err(store_error)?
            else {
                continue;
            };
            let baseline = store.get_baseline_file(&binding.id).map_err(store_error)?;
            if let Some(seed) = discovery::managed_binding_diagnostic(
                binding,
                &definition,
                baseline.as_ref(),
                &projects,
            ) {
                result.diagnostics.push(seed);
            }
        }
        let observations =
            discovery::attach_bindings(result.observed, &bindings, &paths, &projects);
        let observation_ids: Vec<String> = observations.iter().map(|_| new_id("obs")).collect();
        let diagnostics =
            discovery::finalize_diagnostics(result.diagnostics, &observation_ids, &mut new_id);
        let scan_id = new_id("scan");
        let scanned_at = now();
        cache_latest_scan(&scan_id, &diagnostics);
        let observation_rows: Vec<ObservedExtensionDto> = observations
            .iter()
            .zip(observation_ids.iter())
            .map(|(observed, id)| {
                let actions = super::actions::observation_actions(&store, observed, &paths);
                observation_view(id.clone(), observed, actions, &projects)
            })
            .collect();
        cache_discovery_observations(observations, observation_ids);
        Ok(ExtensionDiscoveryDto {
            scan_id,
            scanned_at,
            observations: observation_rows,
            diagnostics,
        })
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtensionDiscoveryDto {
    scan_id: String,
    /// RFC 3339 UTC time of this scan.
    scanned_at: String,
    observations: Vec<ObservedExtensionDto>,
    diagnostics: Vec<ExtensionDiagnostic>,
}

/// Renderer-safe discovery view. Absolute skill/document paths, source
/// identities, and project roots remain in the command cache and never
/// cross IPC.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObservedExtensionDto {
    observation_id: String,
    kind: ExtensionKind,
    client: AppKind,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    origin: ObservedOriginViewDto,
    managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    transport: Option<String>,
    actions: super::actions::ObservationActionsDto,
}

#[derive(Serialize)]
#[serde(
    tag = "origin",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum ObservedOriginViewDto {
    UserRoot,
    LegacyRoot,
    ProjectRoot { project_id: String },
    Managed,
}

pub(super) fn origin_view(
    origin: &ObservedOrigin,
    projects: &[ProjectRegistration],
) -> ObservedOriginViewDto {
    match origin {
        ObservedOrigin::UserRoot => ObservedOriginViewDto::UserRoot,
        ObservedOrigin::LegacyRoot { .. } => ObservedOriginViewDto::LegacyRoot,
        ObservedOrigin::ProjectRoot { project_path } => ObservedOriginViewDto::ProjectRoot {
            project_id: projects
                .iter()
                .find(|project| project.root == *project_path)
                .expect("project-root observations always come from registered projects")
                .id
                .clone(),
        },
        ObservedOrigin::Managed { .. } => ObservedOriginViewDto::Managed,
    }
}

pub(super) fn observation_view(
    observation_id: String,
    observed: &ObservedExtension,
    actions: super::actions::ObservationActionsDto,
    projects: &[ProjectRegistration],
) -> ObservedExtensionDto {
    ObservedExtensionDto {
        observation_id,
        kind: observed.kind,
        client: observed.client,
        name: observed.name.clone(),
        description: observed.description.clone(),
        origin: origin_view(&observed.origin, projects),
        managed: !observed.managed_binding_ids.is_empty(),
        content_digest: observed.content_digest.clone(),
        transport: observed.transport.clone(),
        actions,
    }
}

fn cache_discovery_observations(
    observations: Vec<ObservedExtension>,
    observation_ids: Vec<String>,
) {
    let mut cache = discovered_observations().lock().expect("observations");
    cache.clear();
    for (observed, observation_id) in observations.into_iter().zip(observation_ids) {
        let document_digest = (observed.kind == ExtensionKind::Mcp)
            .then(|| fs::read(&observed.path).ok().map(|bytes| sha_hex(&bytes)))
            .flatten();
        cache.insert(
            observation_id,
            CachedDiscoveryObservation {
                observed,
                document_digest,
            },
        );
    }
}

fn cache_latest_scan(scan_id: &str, diagnostics: &[ExtensionDiagnostic]) {
    let cache = latest_discovery_scan();
    let mut latest = cache.lock().expect("scan");
    *latest = Some(CachedDiscoveryScan {
        scan_id: scan_id.to_string(),
        diagnostics: diagnostics
            .iter()
            .map(|diagnostic| {
                let binding_id = match &diagnostic.subject {
                    asb_core::extensions::diagnostics::DiagnosticSubject::ManagedBinding {
                        binding_id,
                    } => Some(binding_id.clone()),
                    _ => None,
                };
                (
                    diagnostic.id.clone(),
                    CachedDiagnostic {
                        remediation: diagnostic.remediation.clone(),
                        binding_id,
                    },
                )
            })
            .collect(),
    });
}
