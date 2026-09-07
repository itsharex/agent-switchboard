//! The one-click repair entry: it validates that the caller's scan is the
//! latest one and that every named diagnostic is auto-repairable, then
//! produces the same kind of preview plan every other write uses. Actual
//! writes always go through `apply_extension_plan`.

use serde::Deserialize;
use tauri::AppHandle;

use super::planner::{build_repair_operations, stage_plan, Planner};
use super::support::*;
use crate::commands::error::{blocking, state, CommandError};
use crate::extensions::discovery::DiscoveredPaths;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepairRequest {
    scan_id: String,
    diagnostic_ids: Vec<String>,
}

#[tauri::command]
pub async fn prepare_extension_repair(
    app: AppHandle,
    request: RepairRequest,
) -> Result<asb_core::extensions::plan::ExtensionPlanView, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let store = extension_store(&state);
        let latest = latest_discovery_scan().lock().expect("scan").clone();
        let scan = match latest {
            Some(scan) if scan.scan_id == request.scan_id => scan,
            _ => {
                return Err(CommandError::new(
                    "extension-stale-scan",
                    "扫描结果已过期或不存在；请重新扫描后再修复",
                ))
            }
        };
        if request.diagnostic_ids.is_empty() {
            return Err(CommandError::new(
                "extension-invalid",
                "没有选择任何要修复的警告",
            ));
        }
        // Resolve diagnostics to bindings in request order, rejecting
        // anything that is not an auto-repairable managed-object problem.
        let mut binding_ids: Vec<String> = Vec::new();
        for diagnostic_id in &request.diagnostic_ids {
            let diagnostic = scan.diagnostics.get(diagnostic_id).ok_or_else(|| {
                CommandError::new("extension-stale-scan", "警告不属于当前扫描结果；请重新扫描")
            })?;
            match (&diagnostic.remediation, &diagnostic.binding_id) {
                (
                    asb_core::extensions::diagnostics::DiagnosticRemediation::Auto { .. },
                    Some(binding_id),
                ) => {
                    if !binding_ids.contains(binding_id) {
                        binding_ids.push(binding_id.clone());
                    }
                }
                _ => {
                    return Err(CommandError::new(
                        "extension-invalid",
                        "所选警告不能自动修复；请在展开的列表中查看其处理方式",
                    ))
                }
            }
        }
        let paths = DiscoveredPaths::from_env()
            .map_err(|error| CommandError::new("app-state-unavailable", error))?;
        // Repair restores recorded state verbatim; it never resolves or
        // writes secrets, so the resolver is intentionally absent.
        let absent_secrets = |_reference: &str| -> Option<String> { None };
        let planner = Planner {
            store: &store,
            paths: &paths,
            secrets: &absent_secrets,
        };
        let operations = build_repair_operations(&planner, &binding_ids)?;
        let plan = asb_core::extensions::plan::ExtensionPlan {
            plan_id: String::new(),
            created_at: String::new(),
            operations,
            preconditions: asb_core::extensions::plan::PlanPreconditions {
                expires_at: String::new(),
                generation: 0,
                secret_digests: Default::default(),
            },
        };
        stage_plan(&store, plan)
    })
    .await
}
