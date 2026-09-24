//! Codex 项目方案命令（E06）：命名快照的增删改与「预览 → 确认应用」事务。
//!
//! 本模块只拥有方案库本身；应用时的每一步都交给该资源域自己的所有者：
//! 供应商走切换执行器（`switch_provider_internal`），MCP/Skills 走扩展计划
//! 管线（`apply_binding_states`），指令走指令激活事务
//! （项目方案内的直接激活事务）。任一步失败只作为该步
//! 的告警继续，不整体回滚——与 CC 一致，且不会留下「方案库新、live 旧」的
//! 假状态：方案库指针在写入任何客户端文件之前先落盘，失败即早退。
//!
//! Claude 的项目方案由 `commands/claude_project_plans.rs` 独立实现，两边不
//! 共享业务文件、不互相读写。

use super::{
    error::{blocking, require_write_confirmation, state, CommandError},
    ConfigWriteGate,
};
use crate::codex_prompts;
use crate::codex_project_plans::{
    self, ApplyPlan, CodexBindingFact, CodexCurrentState, CodexProjectPlan,
};
use crate::gateway::GatewayController;
use crate::local_state::LocalState;
use crate::runtime_log::RuntimeLogAction;
use asb_core::contracts::AppKind;
use serde::Serialize;
use std::collections::BTreeSet;
use tauri::{AppHandle, Manager};

fn plan_error(message: String) -> CommandError {
    CommandError::new("codex-project-plan-failed", message)
}

async fn activate_project_prompt(app: AppHandle, preset_id: String) -> Result<(), CommandError> {
    let state = state(&app)?;
    let gate = app.state::<ConfigWriteGate>().inner().clone();
    blocking(move || {
        let _guard = gate.lock().map_err(plan_error)?;
        let target = state
            .global_prompt_target(AppKind::Codex)
            .map_err(plan_error)?;
        let prompts = codex_prompts::list(state.root(), &target).map_err(plan_error)?;
        let preview = codex_prompts::preview(state.root(), &target, Some(preset_id), &prompts.revision)
            .map_err(plan_error)?;
        codex_prompts::activate(state.root(), &target, &state.prompt_backup_dir(), preview.plan)
            .map_err(plan_error)?;
        Ok(())
    })
    .await
}

/// Reads the plan library, refusing a caller whose revision is stale. Every
/// mutation goes through this so a preview taken before an edit elsewhere can
/// never overwrite it.
fn load_checked(
    root: &std::path::Path,
    expected: &str,
) -> Result<(Vec<CodexProjectPlan>, Option<String>, String), CommandError> {
    let (plans, current, revision) = codex_project_plans::load(root).map_err(plan_error)?;
    if revision != expected {
        return Err(CommandError::keyed(
            "codex-project-plan-stale",
            "errors.cfg.planLibraryStale",
            "Codex 项目方案库已变化，请刷新后重试",
        ));
    }
    Ok((plans, current, revision))
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexProjectProviderOption {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexProjectPromptOption {
    pub(crate) id: String,
    pub(crate) name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexProjectPlansView {
    pub(crate) plans: Vec<CodexProjectPlan>,
    pub(crate) current: Option<String>,
    pub(crate) revision: String,
    pub(crate) active_provider_id: Option<String>,
    pub(crate) active_prompt_id: Option<String>,
    pub(crate) bindings: Vec<CodexBindingFact>,
    /// Names for the pickers and for reading a slot back; ids alone are not
    /// presentable and the renderer never resolves them itself.
    pub(crate) providers: Vec<CodexProjectProviderOption>,
    pub(crate) prompts: Vec<CodexProjectPromptOption>,
}

/// One step the apply will take, phrased for the confirmation sheet. Never
/// carries a key, a path, or client file text.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexProjectApplyStep {
    /// `provider` | `mcp` | `skill` | `prompt`.
    pub(crate) kind: &'static str,
    /// Provider id, definition id, or preset id.
    pub(crate) target: String,
    pub(crate) label: String,
    /// `switch` | `enable` | `disable` | `activate`.
    pub(crate) action: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexProjectApplyPreview {
    pub(crate) plan_id: String,
    pub(crate) steps: Vec<CodexProjectApplyStep>,
    pub(crate) warnings: Vec<asb_core::contracts::LocalizedMessage>,
    /// The plan that will be re-snapshotted from the live state before this
    /// one is applied, matching the CC "autosave the old project" semantics.
    pub(crate) autosave_plan_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexProjectApplyOutcome {
    pub(crate) steps: Vec<CodexProjectApplyStep>,
    pub(crate) warnings: Vec<asb_core::contracts::LocalizedMessage>,
    pub(crate) view: CodexProjectPlansView,
}

fn gateway_of(app: &AppHandle) -> GatewayController {
    app.state::<GatewayController>().inner().clone()
}

fn build_view(
    plans: Vec<CodexProjectPlan>,
    current: Option<String>,
    revision: String,
    snapshot: &CodexCurrentState,
    providers: Vec<CodexProjectProviderOption>,
    prompts: Vec<CodexProjectPromptOption>,
) -> CodexProjectPlansView {
    CodexProjectPlansView {
        plans,
        current,
        revision,
        active_provider_id: snapshot.active_provider_id.clone(),
        active_prompt_id: snapshot.active_prompt_id.clone(),
        bindings: snapshot.facts.clone(),
        providers,
        prompts,
    }
}

/// Everything one apply needs, resolved once so the preview and the commit
/// cannot drift apart.
struct Prepared {
    plans: Vec<CodexProjectPlan>,
    revision: String,
    target: CodexProjectPlan,
    snapshot: CodexCurrentState,
    plan: ApplyPlan,
    autosave_plan_id: Option<String>,
    steps: Vec<CodexProjectApplyStep>,
}

fn provider_names(state: &LocalState) -> Result<Vec<CodexProjectProviderOption>, CommandError> {
    Ok(state
        .configuration()
        .list_providers()
        .map_err(|error| CommandError::new("provider-store-unavailable", error.to_string()))?
        .into_iter()
        .map(|record| CodexProjectProviderOption {
            id: record.profile.id,
            name: record.profile.name,
        })
        .collect())
}

fn prompt_names(state: &LocalState) -> Result<Vec<CodexProjectPromptOption>, CommandError> {
    Ok(crate::codex_prompts::list(
        state.root(),
        &state
            .global_prompt_target(AppKind::Codex)
            .map_err(plan_error)?,
    )
    .map_err(plan_error)?
    .presets
    .into_iter()
    .map(|preset| CodexProjectPromptOption {
        id: preset.id,
        name: preset.draft.name,
    })
    .collect())
}

fn prepare(
    app: &AppHandle,
    plan_id: &str,
    expected_revision: Option<&str>,
) -> Result<Prepared, CommandError> {
    let state = state(app)?;
    let (plans, current, revision) = codex_project_plans::load(state.root()).map_err(plan_error)?;
    if let Some(expected) = expected_revision {
        if expected != revision {
            return Err(CommandError::keyed(
                "codex-project-plan-stale",
                "errors.cfg.planLibraryStale",
                "Codex 项目方案库已变化，请刷新后重试",
            ));
        }
    }
    let target = plans
        .iter()
        .find(|plan| plan.id == plan_id)
        .cloned()
        .ok_or_else(|| {
            CommandError::localized(
                "codex-project-plan-missing",
                "errors.cfg.planMissing",
                format!("项目方案 {plan_id} 不存在"),
                serde_json::json!({ "planId": plan_id }),
            )
        })?;

    let snapshot = codex_project_plans::snapshot_current(&state, &gateway_of(app))
        .map_err(plan_error)?;
    let providers = provider_names(&state)?;
    let provider_ids: BTreeSet<&str> = providers.iter().map(|option| option.id.as_str()).collect();

    let prompt_view = crate::codex_prompts::list(
        state.root(),
        &state
            .global_prompt_target(AppKind::Codex)
            .map_err(plan_error)?,
    )
    .map_err(plan_error)?;
    let prompt_ids: BTreeSet<&str> = prompt_view
        .presets
        .iter()
        .map(|preset| preset.id.as_str())
        .collect();

    let provider_exists = match target.slot.providers.as_deref() {
        Some(id) if !id.is_empty() => provider_ids.contains(id),
        _ => true,
    };
    let prompt_exists = match target.slot.prompts.as_deref() {
        Some(id) if !id.is_empty() => prompt_ids.contains(id),
        _ => true,
    };

    let plan = codex_project_plans::compute_apply_plan(
        &target.slot,
        snapshot.active_provider_id.as_deref(),
        provider_exists,
        &codex_project_plans::definition_states_by_kind(&snapshot.facts, "mcp"),
        &codex_project_plans::definition_states_by_kind(&snapshot.facts, "skill"),
        snapshot.active_prompt_id.as_deref(),
        prompt_exists,
    );

    let name_of = |wanted: &str| -> String {
        snapshot
            .facts
            .iter()
            .find(|fact| fact.definition_id == wanted)
            .map(|fact| fact.name.clone())
            .unwrap_or_else(|| wanted.to_string())
    };
    let provider_label = |wanted: &str| -> String {
        providers
            .iter()
            .find(|option| option.id == wanted)
            .map(|option| option.name.clone())
            .unwrap_or_else(|| wanted.to_string())
    };

    let mut steps = Vec::new();
    if let Some(id) = plan.provider_switch.as_deref() {
        steps.push(CodexProjectApplyStep {
            kind: "provider",
            target: id.to_string(),
            label: provider_label(id),
            action: "switch",
        });
    }
    for (definition_id, enabled) in plan.mcp_toggles.iter().chain(plan.skills_toggles.iter()) {
        let kind = if plan.mcp_toggles.iter().any(|(id, _)| id == definition_id) {
            "mcp"
        } else {
            "skill"
        };
        steps.push(CodexProjectApplyStep {
            kind,
            target: definition_id.clone(),
            label: name_of(definition_id),
            action: if *enabled { "enable" } else { "disable" },
        });
    }
    if let Some(id) = plan.prompt_activate.as_deref() {
        steps.push(CodexProjectApplyStep {
            kind: "prompt",
            target: id.to_string(),
            label: prompt_view
                .presets
                .iter()
                .find(|preset| preset.id == id)
                .map(|preset| preset.draft.name.clone())
                .unwrap_or_else(|| id.to_string()),
            action: "activate",
        });
    }

    let autosave_plan_id = current
        .as_deref()
        .filter(|current_id| *current_id != plan_id)
        .map(str::to_string);

    Ok(Prepared {
        plans,
        revision,
        target,
        snapshot,
        plan,
        autosave_plan_id,
        steps,
    })
}

fn read_view(app: &AppHandle) -> Result<CodexProjectPlansView, CommandError> {
    let state = state(app)?;
    let (plans, current, revision) = codex_project_plans::load(state.root()).map_err(plan_error)?;
    let snapshot =
        codex_project_plans::snapshot_current(&state, &gateway_of(app)).map_err(plan_error)?;
    Ok(build_view(
        plans,
        current,
        revision,
        &snapshot,
        provider_names(&state)?,
        prompt_names(&state)?,
    ))
}

#[tauri::command]
pub(crate) async fn list_codex_project_plans(
    app: AppHandle,
) -> Result<CodexProjectPlansView, CommandError> {
    blocking(move || read_view(&app)).await
}

#[tauri::command]
pub(crate) async fn create_codex_project_plan(
    app: AppHandle,
    name: String,
    expected_revision: String,
) -> Result<CodexProjectPlansView, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(CommandError::keyed(
                "codex-project-plan-name-empty",
                "errors.cfg.planNameEmpty",
                "项目方案名称不能为空",
            ));
        }
        let (mut plans, current, revision) = load_checked(state.root(), &expected_revision)?;
        let snapshot = codex_project_plans::snapshot_current(&state, &gateway_of(&app))
            .map_err(plan_error)?;
        plans.push(CodexProjectPlan {
            id: format!("project-{}", uuid::Uuid::new_v4().simple()),
            name,
            slot: snapshot.slot,
            updated_at: codex_project_plans::now_rfc3339(),
        });
        codex_project_plans::save(state.root(), plans, current, &revision).map_err(plan_error)?;
        read_view(&app)
    })
    .await
}

#[tauri::command]
pub(crate) async fn rename_codex_project_plan(
    app: AppHandle,
    plan_id: String,
    name: String,
    expected_revision: String,
) -> Result<CodexProjectPlansView, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(CommandError::keyed(
                "codex-project-plan-name-empty",
                "errors.cfg.planNameEmpty",
                "项目方案名称不能为空",
            ));
        }
        let (mut plans, current, revision) = load_checked(state.root(), &expected_revision)?;
        let target = plans
            .iter_mut()
            .find(|plan| plan.id == plan_id)
            .ok_or_else(|| {
                CommandError::localized(
                    "codex-project-plan-missing",
                    "errors.cfg.planMissing",
                    format!("项目方案 {plan_id} 不存在"),
                    serde_json::json!({ "planId": plan_id }),
                )
            })?;
        target.name = name;
        target.updated_at = codex_project_plans::now_rfc3339();
        codex_project_plans::save(state.root(), plans, current, &revision).map_err(plan_error)?;
        read_view(&app)
    })
    .await
}

/// Re-captures the Codex slot from the live state. Only the Codex group is
/// touched; the other clients' groups live in their own stores.
#[tauri::command]
pub(crate) async fn resnapshot_codex_project_plan(
    app: AppHandle,
    plan_id: String,
    expected_revision: String,
) -> Result<CodexProjectPlansView, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let (mut plans, current, revision) = load_checked(state.root(), &expected_revision)?;
        let snapshot = codex_project_plans::snapshot_current(&state, &gateway_of(&app))
            .map_err(plan_error)?;
        let target = plans
            .iter_mut()
            .find(|plan| plan.id == plan_id)
            .ok_or_else(|| {
                CommandError::localized(
                    "codex-project-plan-missing",
                    "errors.cfg.planMissing",
                    format!("项目方案 {plan_id} 不存在"),
                    serde_json::json!({ "planId": plan_id }),
                )
            })?;
        target.slot = snapshot.slot;
        target.updated_at = codex_project_plans::now_rfc3339();
        codex_project_plans::save(state.root(), plans, current, &revision).map_err(plan_error)?;
        read_view(&app)
    })
    .await
}

#[tauri::command]
pub(crate) async fn delete_codex_project_plan(
    app: AppHandle,
    plan_id: String,
    expected_revision: String,
) -> Result<CodexProjectPlansView, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let (mut plans, current, revision) = load_checked(state.root(), &expected_revision)?;
        let before = plans.len();
        plans.retain(|plan| plan.id != plan_id);
        if plans.len() == before {
            return Err(CommandError::localized(
                "codex-project-plan-missing",
                "errors.cfg.planMissing",
                format!("项目方案 {plan_id} 不存在"),
                serde_json::json!({ "planId": plan_id }),
            ));
        }
        // Deleting the bound project clears only the Codex pointer.
        let current = current.filter(|current_id| *current_id != plan_id);
        codex_project_plans::save(state.root(), plans, current, &revision).map_err(plan_error)?;
        read_view(&app)
    })
    .await
}

/// Moves (or clears) the Codex project pointer without touching any
/// configuration. `plan_id = None` unbinds.
#[tauri::command]
pub(crate) async fn set_current_codex_project_plan(
    app: AppHandle,
    plan_id: Option<String>,
    expected_revision: String,
) -> Result<CodexProjectPlansView, CommandError> {
    blocking(move || {
        let state = state(&app)?;
        let (plans, _, revision) = load_checked(state.root(), &expected_revision)?;
        if let Some(id) = plan_id.as_deref() {
            if !plans.iter().any(|plan| plan.id == id) {
                return Err(CommandError::localized(
                    "codex-project-plan-missing",
                    "errors.cfg.planMissing",
                    format!("项目方案 {id} 不存在"),
                    serde_json::json!({ "planId": id }),
                ));
            }
        }
        codex_project_plans::save(state.root(), plans, plan_id, &revision).map_err(plan_error)?;
        read_view(&app)
    })
    .await
}

#[tauri::command]
pub(crate) async fn preview_codex_project_plan_apply(
    app: AppHandle,
    plan_id: String,
    expected_revision: String,
) -> Result<CodexProjectApplyPreview, CommandError> {
    blocking(move || {
        let prepared = prepare(&app, &plan_id, Some(&expected_revision))?;
        Ok(CodexProjectApplyPreview {
            plan_id,
            steps: prepared.steps,
            warnings: prepared.plan.warnings,
            autosave_plan_id: prepared.autosave_plan_id,
        })
    })
    .await
}

/// Applies one project plan: the pointer and the autosave of the previous
/// project land first (one CAS write, no client file touched yet), then each
/// owner runs its own backed-up transaction. A failing step is reported and
/// the rest continue.
#[tauri::command]
pub(crate) async fn apply_codex_project_plan(
    app: AppHandle,
    plan_id: String,
    expected_revision: String,
    confirm_write: bool,
) -> Result<CodexProjectApplyOutcome, CommandError> {
    require_write_confirmation(confirm_write, "应用 Codex 项目方案")?;
    crate::commands::error::observe(RuntimeLogAction::ConfigurationSwitched, async move {
        let prepared = blocking({
            let app = app.clone();
            let plan_id = plan_id.clone();
            let expected_revision = expected_revision.clone();
            move || prepare(&app, &plan_id, Some(&expected_revision))
        })
        .await?;

        // Bind the pointer and autosave the outgoing project in one revision
        // -checked write, before any client file can change.
        let revision = prepared.revision.clone();
        let mut plans = prepared.plans.clone();
        if let Some(current_id) = prepared.autosave_plan_id.as_deref() {
            if let Some(outgoing) = plans.iter_mut().find(|plan| plan.id == current_id) {
                outgoing.slot = prepared.snapshot.slot.clone();
                outgoing.updated_at = codex_project_plans::now_rfc3339();
            }
        }
        let target_id = prepared.target.id.clone();
        blocking({
            let app = app.clone();
            move || {
                let state = state(&app)?;
                codex_project_plans::save(state.root(), plans, Some(target_id), &revision)
                    .map_err(plan_error)
            }
        })
        .await?;

        let mut warnings = prepared.plan.warnings.clone();
        let mut applied = Vec::new();

        // 1. Provider switch, through the one owner of client-file writes.
        if let Some(provider_id) = prepared.plan.provider_switch.as_deref() {
            match crate::commands::switching::switch_provider_internal(
                app.clone(),
                provider_id.to_string(),
            )
            .await
            {
                Ok(outcome) => {
                    warnings.extend(outcome.warnings);
                    applied.push(prepared.steps.iter().find(|step| step.kind == "provider").cloned());
                }
                Err(error) => warnings.push(asb_core::contracts::LocalizedMessage::new(
                    "errors.cfg.applyProviderSwitchFailed",
                    serde_json::json!({ "providerId": provider_id, "detail": error.message }),
                    format!("供应商切换失败（{provider_id}）：{}", error.message),
                )),
            }
        }

        // 2. MCP and Skills, through the extension plan pipeline so every
        //    write stays journaled, backed up, and target-locked.
        for step in &prepared.steps {
            if step.kind != "mcp" && step.kind != "skill" {
                continue;
            }
            let enable = step.action == "enable";
            let toggles: Vec<(String, bool)> =
                codex_project_plans::bindings_for_definition(
                    &prepared.snapshot.facts,
                    &step.target,
                    step.kind,
                    enable,
                )
                .into_iter()
                .map(|binding_id| (binding_id, enable))
                .collect();
            if toggles.is_empty() {
                continue;
            }
            match crate::commands::extensions::apply_binding_states(app.clone(), toggles)
                .await
            {
                Ok(()) => applied.push(Some(step.clone())),
                Err(error) => {
                    let kind_label = if step.kind == "mcp" { "MCP" } else { "Skill" };
                let (key, description) = if enable {
                    (
                        "errors.cfg.applyBindingEnableFailed",
                        format!("{kind_label} {} 启用失败：{}", step.label, error.message),
                    )
                } else {
                    (
                        "errors.cfg.applyBindingDisableFailed",
                        format!("{kind_label} {} 停用失败：{}", step.label, error.message),
                    )
                };
                    warnings.push(asb_core::contracts::LocalizedMessage::new(
                        key,
                        serde_json::json!({
                            "kind": kind_label,
                            "label": step.label,
                            "detail": error.message,
                        }),
                        description,
                    ));
                }
            }
        }

        // 3. Prompt activation stays inside the project-plan transaction.
        if let Some(preset_id) = prepared.plan.prompt_activate.as_deref() {
            match activate_project_prompt(app.clone(), preset_id.to_string()).await {
                Ok(()) => applied.push(
                    prepared
                        .steps
                        .iter()
                        .find(|step| step.kind == "prompt")
                        .cloned(),
                ),
                Err(error) => warnings.push(asb_core::contracts::LocalizedMessage::new(
                    "errors.cfg.applyPromptActivateFailed",
                    serde_json::json!({ "presetId": preset_id, "detail": error.message }),
                    format!("指令预设 {preset_id} 激活失败：{}", error.message),
                )),
            }
        }

        let view = blocking({
            let app = app.clone();
            move || read_view(&app)
        })
        .await?;
        Ok(CodexProjectApplyOutcome {
            steps: applied.into_iter().flatten().collect(),
            warnings,
            view,
        })
    })
    .await
}
