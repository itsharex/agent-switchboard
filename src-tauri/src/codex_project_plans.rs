//! Codex 项目方案（E06）：供应商 + MCP + Skills + 指令预设的命名联动快照。
//! 本模块是方案库的唯一所有者；应用时不自己写客户端文件，而是逐项调用各
//! 资源域自己的所有者接口（切换执行器、扩展计划管线、指令激活事务），
//! best-effort 逐项报告失败。槽位语义与 CC 严格一致：`None` = 从未拍过
//! 快照（应用时不动），与「拍到的就是空集」（应用时清空启用）严格区分；
//! 供应商与指令槽位用空串表达「拍到时没有激活项」。Claude 分组零触碰。
//!
//! 与 Claude 的实现完全分开：Claude 的项目方案驻留 `claude_project_plans`，
//! 两边的方案库、标记与编排各归各的文件，互不读取。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use asb_core::extensions::{DesiredState, ExtensionKind, ExtensionTarget};
use asb_core::AppKind;
use asb_switch::sha256_hex;
use serde::{Deserialize, Serialize};

fn lock() -> Result<MutexGuard<'static, ()>, String> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Codex 项目方案锁不可用，请重启应用".into())
}

/// One Codex project slot. Every field is an `Option` so "never captured"
/// and "captured as empty" stay strictly distinguishable.
///
/// `providers` / `prompts` additionally use the empty string as the
/// "captured while nothing was active" marker: `Some("")` is a captured
/// slot that must not switch or activate anything, while `None` means the
/// scope was never captured at all.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub(crate) struct CodexProjectSlot {
    pub(crate) providers: Option<String>,
    pub(crate) mcp: Option<Vec<String>>,
    pub(crate) skills: Option<Vec<String>>,
    pub(crate) prompts: Option<String>,
}

impl CodexProjectSlot {
    pub(crate) fn scope_captured(&self) -> bool {
        self.providers.is_some()
            || self.mcp.is_some()
            || self.skills.is_some()
            || self.prompts.is_some()
    }

    /// 最小 toggle 集：从 `current`（id → 是否启用）到目标集合，返回需要执行
    /// 的 `(id, 目标态)` 与目标中已不存在的悬空 id。
    pub(crate) fn plan_toggles(
        current: &[(String, bool)],
        target_ids: &[String],
    ) -> (Vec<(String, bool)>, Vec<String>) {
        let existing: BTreeSet<&str> = current.iter().map(|(id, _)| id.as_str()).collect();
        let target: BTreeSet<&str> = target_ids.iter().map(|s| s.as_str()).collect();
        let toggles = current
            .iter()
            .filter(|(id, enabled)| target.contains(id.as_str()) != *enabled)
            .map(|(id, enabled)| (id.clone(), !*enabled))
            .collect();
        let dangling = target_ids
            .iter()
            .filter(|id| !existing.contains(id.as_str()))
            .cloned()
            .collect();
        (toggles, dangling)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CodexProjectPlan {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) slot: CodexProjectSlot,
    pub(crate) updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
struct ProjectPlansFile {
    version: u8,
    plans: Vec<CodexProjectPlan>,
    /// The Codex group's current project pointer; `None` = nothing bound.
    current: Option<String>,
}

fn store_path(root: &Path) -> PathBuf {
    root.join("codex-project-plans.json")
}

pub(crate) fn load(root: &Path) -> Result<(Vec<CodexProjectPlan>, Option<String>, String), String> {
    let _guard = lock()?;
    let raw = match fs::read_to_string(store_path(root)) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), None, sha256_hex("[]")))
        }
        Err(_) => return Err("无法读取 Codex 项目方案库".into()),
    };
    let file: ProjectPlansFile = serde_json::from_str(&raw)
        .map_err(|_| "Codex 项目方案库格式无效，请从备份恢复".to_string())?;
    Ok((file.plans, file.current, sha256_hex(&raw)))
}

pub(crate) fn save(
    root: &Path,
    plans: Vec<CodexProjectPlan>,
    current: Option<String>,
    expected: &str,
) -> Result<String, String> {
    let _guard = lock()?;
    let text = serde_json::to_string_pretty(&ProjectPlansFile {
        version: 1,
        plans,
        current,
    })
    .map_err(|_| "Codex 项目方案库无法序列化".to_string())?;
    if current_revision(root)? != expected {
        return Err("Codex 项目方案库已变化，请刷新后重试".into());
    }
    let path = store_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "无法创建 Codex 项目方案目录".to_string())?;
    }
    let temporary = path.with_extension("json.tmp");
    let result = (|| {
        fs::write(&temporary, text.as_bytes()).map_err(|_| "无法写入临时文件".to_string())?;
        fs::rename(&temporary, &path).map_err(|_| "无法原子替换项目方案库".to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map(|_| sha256_hex(&text))
}

fn current_revision(root: &Path) -> Result<String, String> {
    match fs::read_to_string(store_path(root)) {
        Ok(raw) => Ok(sha256_hex(&raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(sha256_hex("[]")),
        Err(_) => Err("无法读取 Codex 项目方案库".into()),
    }
}

pub(crate) fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// One Codex-scoped extension binding, as both the picker and the apply plan
/// see it. Bindings are the client-scoped unit the extension plan pipeline
/// toggles, so the fact carries the binding id while the slot stores the
/// (stable) definition id.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexBindingFact {
    pub(crate) binding_id: String,
    pub(crate) definition_id: String,
    pub(crate) name: String,
    /// `"mcp"` or `"skill"`.
    pub(crate) kind: &'static str,
    /// Human-readable target description; never a filesystem path.
    pub(crate) scope: String,
    pub(crate) enabled: bool,
}

fn kind_label(kind: ExtensionKind) -> &'static str {
    match kind {
        ExtensionKind::Mcp => "mcp",
        ExtensionKind::Skill => "skill",
    }
}

fn scope_label(target: &ExtensionTarget) -> String {
    match target {
        ExtensionTarget::App { .. } => "用户级".to_string(),
        ExtensionTarget::ProjectShared { project_id, .. } => {
            format!("项目共享 · {project_id}")
        }
        ExtensionTarget::ProjectPrivate { project_id, .. } => {
            format!("项目私有 · {project_id}")
        }
    }
}

/// Every Codex-scoped binding in the extension library, ordered MCP first.
pub(crate) fn codex_bindings(
    state: &crate::local_state::LocalState,
) -> Result<Vec<CodexBindingFact>, String> {
    use crate::extensions::store::ExtensionStore;
    let store = ExtensionStore::from_state(state);
    let definitions: BTreeMap<String, (String, ExtensionKind)> = store
        .list_definitions()
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|definition| {
            (
                definition.id.clone(),
                (definition.name.clone(), definition.kind()),
            )
        })
        .collect();
    let mut facts = Vec::new();
    for binding in store
        .list_bindings()
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|binding| binding.target.client() == AppKind::Codex)
    {
        let Some((name, kind)) = definitions.get(&binding.resource_id) else {
            continue;
        };
        facts.push(CodexBindingFact {
            binding_id: binding.id.clone(),
            definition_id: binding.resource_id.clone(),
            name: name.clone(),
            kind: kind_label(*kind),
            scope: scope_label(&binding.target),
            enabled: binding.desired == DesiredState::Enabled,
        });
    }
    facts.sort_by(|left, right| {
        (left.kind, &left.name, &left.binding_id).cmp(&(right.kind, &right.name, &right.binding_id))
    });
    Ok(facts)
}

/// Aggregates bindings to the definition-level `(definition id, enabled)`
/// view the slot stores: a definition counts as enabled when at least one of
/// its Codex bindings is enabled. This mirrors CC's per-app "enabled" flag
/// while keeping the slot free of binding ids that a re-registration can
/// invalidate.
pub(crate) fn definition_states(facts: &[CodexBindingFact]) -> Vec<(String, bool)> {
    let mut states: BTreeMap<&str, bool> = BTreeMap::new();
    for fact in facts {
        let entry = states.entry(fact.definition_id.as_str()).or_insert(false);
        *entry = *entry || fact.enabled;
    }
    states
        .into_iter()
        .map(|(id, enabled)| (id.to_string(), enabled))
        .collect()
}

/// Same aggregation, restricted to one extension kind, so the apply plan
/// never mixes MCP toggles into the Skill diff.
pub(crate) fn definition_states_by_kind(
    facts: &[CodexBindingFact],
    kind: &str,
) -> Vec<(String, bool)> {
    definition_states(
        &facts
            .iter()
            .filter(|fact| fact.kind == kind)
            .cloned()
            .collect::<Vec<_>>(),
    )
}

/// The binding ids to toggle so that `definition_id` ends up `enabled`,
/// restricted to one extension kind.
pub(crate) fn bindings_for_definition(
    facts: &[CodexBindingFact],
    definition_id: &str,
    kind: &str,
    enabled: bool,
) -> Vec<String> {
    facts
        .iter()
        .filter(|fact| {
            fact.definition_id == definition_id && fact.kind == kind && fact.enabled != enabled
        })
        .map(|fact| fact.binding_id.clone())
        .collect()
}

/// Reads the current Codex state for one scope. `mcp` and `skills` are always
/// `Some` (possibly empty) so a captured scope is never mistaken for an
/// uncaptured one, matching CC.
#[derive(Debug, Clone)]
pub(crate) struct CodexCurrentState {
    pub(crate) slot: CodexProjectSlot,
    pub(crate) facts: Vec<CodexBindingFact>,
    pub(crate) active_provider_id: Option<String>,
    pub(crate) active_prompt_id: Option<String>,
}

pub(crate) fn snapshot_current(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
) -> Result<CodexCurrentState, String> {
    let active_provider_id = active_codex_provider(state, gateway)?;
    let facts = codex_bindings(state)?;
    let enabled_by_kind = |want: &str| -> Vec<String> {
        facts
            .iter()
            .filter(|fact| fact.kind == want && fact.enabled)
            .map(|fact| fact.definition_id.clone())
            .collect::<BTreeSet<String>>()
            .into_iter()
            .collect()
    };
    let active_prompt_id =
        crate::codex_prompts::list(state.root(), &state.global_prompt_target(AppKind::Codex)?)?
            .active_id;
    Ok(CodexCurrentState {
        slot: CodexProjectSlot {
            providers: Some(active_provider_id.clone().unwrap_or_default()),
            mcp: Some(enabled_by_kind("mcp")),
            skills: Some(enabled_by_kind("skill")),
            prompts: Some(active_prompt_id.clone().unwrap_or_default()),
        },
        facts,
        active_provider_id,
        active_prompt_id,
    })
}

/// The active Codex provider identity, from the single owner of that fact.
fn active_codex_provider(
    state: &crate::local_state::LocalState,
    gateway: &crate::gateway::GatewayController,
) -> Result<Option<String>, String> {
    crate::commands::status::config_status_report(state, gateway)
        .map_err(|error| error.message)?
        .into_iter()
        .find(|status| status.app == AppKind::Codex)
        .map(|status| status.active_profile_id)
        .ok_or_else(|| "无法读取 Codex 配置状态".to_string())
}

/// 编排决策（纯函数）：给定快照槽位与当前各域状态，产出逐步执行计划与
/// 逐项警告。执行由命令层调用各资源域自己的所有者接口完成。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ApplyPlan {
    /// 需要切换到的供应商 id（当前已是指向时为 None）。
    pub(crate) provider_switch: Option<String>,
    /// MCP 最小 toggle 集 `(definition id, 目标态)`。
    pub(crate) mcp_toggles: Vec<(String, bool)>,
    /// Skill 最小 toggle 集。
    pub(crate) skills_toggles: Vec<(String, bool)>,
    /// 需要激活的指令预设 id（已激活时为 None）。
    pub(crate) prompt_activate: Option<String>,
    pub(crate) warnings: Vec<asb_core::contracts::LocalizedMessage>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_apply_plan(
    slot: &CodexProjectSlot,
    provider_active: Option<&str>,
    provider_exists: bool,
    mcp_current: &[(String, bool)],
    skills_current: &[(String, bool)],
    prompt_active: Option<&str>,
    prompt_exists: bool,
) -> ApplyPlan {
    let mut warnings = Vec::new();
    if !slot.scope_captured() {
        warnings.push(asb_core::contracts::LocalizedMessage::new(
            "errors.cfg.planNeverCaptured",
            serde_json::json!({}),
            "该项目方案尚未拍过 Codex 快照；已标记为当前项目且未改动任何配置，切走时会自动补拍。",
        ));
        return ApplyPlan {
            provider_switch: None,
            mcp_toggles: Vec::new(),
            skills_toggles: Vec::new(),
            prompt_activate: None,
            warnings,
        };
    }

    let provider_switch = match slot.providers.as_deref() {
        // 未拍过，或拍到时没有激活供应商：都不应触发切换。
        None | Some("") => None,
        // 存在性先于「已指向」判定，与 CC 的检查顺序一致：
        // 目标档案已消失时必须具名告警，而不是静默当作已对齐。
        Some(target) if !provider_exists => {
            warnings.push(asb_core::contracts::LocalizedMessage::new(
                "errors.cfg.planProviderGone",
                serde_json::json!({ "target": target }),
                format!("供应商 {target} 已不存在，已跳过供应商切换"),
            ));
            None
        }
        Some(target) if provider_active == Some(target) => None,
        Some(target) => Some(target.to_string()),
    };

    let (mcp_toggles, mcp_dangling) = match &slot.mcp {
        Some(ids) => CodexProjectSlot::plan_toggles(mcp_current, ids),
        None => (Vec::new(), Vec::new()),
    };
    for id in &mcp_dangling {
        warnings.push(asb_core::contracts::LocalizedMessage::new(
                "errors.cfg.planMcpGone",
                serde_json::json!({ "id": id }),
                format!("MCP {id} 已不存在，已跳过"),
            ));
    }
    let (skills_toggles, skills_dangling) = match &slot.skills {
        Some(ids) => CodexProjectSlot::plan_toggles(skills_current, ids),
        None => (Vec::new(), Vec::new()),
    };
    for id in &skills_dangling {
        warnings.push(asb_core::contracts::LocalizedMessage::new(
                "errors.cfg.planSkillGone",
                serde_json::json!({ "id": id }),
                format!("Skill {id} 已不存在，已跳过"),
            ));
    }

    let prompt_activate = match slot.prompts.as_deref() {
        // 未拍过，或拍到时没有激活指令：都不应触发激活。
        None | Some("") => None,
        // 同样先判存在性：预设已删除但槽位仍记录它时必须具名告警。
        Some(id) if !prompt_exists => {
            warnings.push(asb_core::contracts::LocalizedMessage::new(
                "errors.cfg.planPromptGone",
                serde_json::json!({ "id": id }),
                format!("指令预设 {id} 已不存在，已跳过"),
            ));
            None
        }
        Some(id) if prompt_active == Some(id) => None,
        Some(id) => Some(id.to_string()),
    };

    ApplyPlan {
        provider_switch,
        mcp_toggles,
        skills_toggles,
        prompt_activate,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(providers: Option<String>, mcp: Option<Vec<String>>) -> CodexProjectSlot {
        CodexProjectSlot {
            providers,
            mcp,
            skills: None,
            prompts: None,
        }
    }

    fn facts() -> Vec<CodexBindingFact> {
        vec![
            CodexBindingFact {
                binding_id: "bind-a".into(),
                definition_id: "mcp-a".into(),
                name: "server-a".into(),
                kind: "mcp",
                scope: "用户级".into(),
                enabled: true,
            },
            CodexBindingFact {
                binding_id: "bind-a-project".into(),
                definition_id: "mcp-a".into(),
                name: "server-a".into(),
                kind: "mcp",
                scope: "项目共享 · p1".into(),
                enabled: false,
            },
            CodexBindingFact {
                binding_id: "bind-b".into(),
                definition_id: "mcp-b".into(),
                name: "server-b".into(),
                kind: "mcp",
                scope: "用户级".into(),
                enabled: false,
            },
        ]
    }

    #[test]
    fn none_and_empty_slots_are_strictly_distinguishable() {
        let untouched = slot(None, None);
        let captured_empty = slot(Some(String::new()), Some(Vec::new()));
        assert!(!untouched.scope_captured());
        assert!(captured_empty.scope_captured());

        // plan_toggles: an empty captured set disables everything currently on.
        let (toggles, dangling) = CodexProjectSlot::plan_toggles(
            &[("mcp-a".into(), true), ("mcp-b".into(), false)],
            &[],
        );
        assert_eq!(toggles, vec![("mcp-a".into(), false)]);
        assert!(dangling.is_empty());

        // A captured target enables what is off and reports missing ids.
        let (toggles, dangling) = CodexProjectSlot::plan_toggles(
            &[("mcp-a".into(), true), ("mcp-b".into(), false)],
            &["mcp-b".into(), "mcp-gone".into()],
        );
        assert_eq!(
            toggles,
            vec![("mcp-a".into(), false), ("mcp-b".into(), true)]
        );
        assert_eq!(dangling, vec!["mcp-gone".to_string()]);
    }

    #[test]
    fn store_roundtrips_plans_and_current_with_cas() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let (plans, current, revision) = load(root).unwrap();
        assert!(plans.is_empty() && current.is_none());

        let plan = CodexProjectPlan {
            id: "p1".into(),
            name: "工作项目".into(),
            slot: slot(Some("provider-1".into()), Some(vec!["mcp-a".into()])),
            updated_at: now_rfc3339(),
        };
        let next = save(root, vec![plan], Some("p1".into()), &revision).unwrap();
        let (plans, current, _) = load(root).unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(current.as_deref(), Some("p1"));

        // A stale revision must not land.
        let stale = save(
            root,
            Vec::new(),
            None,
            &sha256_hex("[]"),
        );
        assert!(stale.is_err());
        let (plans, current, _) = load(root).unwrap();
        assert_eq!(plans.len(), 1, "stale write must not land");
        assert_eq!(current.as_deref(), Some("p1"));
        let _ = next;
    }

    #[test]
    fn definition_states_aggregate_every_codex_binding() {
        let states = definition_states(&facts());
        assert_eq!(
            states,
            vec![
                ("mcp-a".to_string(), true),
                ("mcp-b".to_string(), false)
            ],
            "同一定义在多个作用域下只要有一个启用即视为启用"
        );
        assert_eq!(
            bindings_for_definition(&facts(), "mcp-a", "mcp", false),
            vec!["bind-a".to_string()],
            "只关闭当前启用的那条绑定，已关闭的项目绑定不重复下发"
        );
        assert_eq!(
            bindings_for_definition(&facts(), "mcp-b", "mcp", true),
            vec!["bind-b".to_string()]
        );
        assert!(bindings_for_definition(&facts(), "mcp-a", "skill", false).is_empty());
    }
}

#[cfg(test)]
mod apply_tests {
    use super::*;

    #[test]
    fn apply_plan_skips_captured_no_ops_and_reports_every_skipped_resource() {
        let slot = CodexProjectSlot {
            providers: Some("provider-1".into()),
            mcp: Some(vec!["mcp-a".into(), "mcp-gone".into()]),
            skills: Some(Vec::new()),
            prompts: Some("prompt-1".into()),
        };
        let plan = compute_apply_plan(
            &slot,
            Some("provider-1"),
            true,
            &[("mcp-a".into(), false), ("mcp-b".into(), true)],
            &[],
            Some("prompt-1"),
            true,
        );
        assert_eq!(plan.provider_switch, None, "已指向目标时零切换");
        assert_eq!(
            plan.mcp_toggles,
            vec![("mcp-a".into(), true), ("mcp-b".into(), false)],
            "最小 diff：开启目标中未启用的，关闭目标外已启用的；已对齐的条目一律不动"
        );
        assert!(plan.skills_toggles.is_empty(), "空集已对齐时零改动");
        assert_eq!(plan.prompt_activate, None, "已激活的指令幂等跳过");
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("mcp-gone")));

        let missing = compute_apply_plan(
            &slot,
            Some("provider-9"),
            false,
            &[],
            &[],
            Some("prompt-1"),
            false,
        );
        assert_eq!(missing.provider_switch, None);
        assert_eq!(missing.prompt_activate, None);
        assert!(missing
            .warnings
            .iter()
            .any(|warning| warning.contains("供应商 provider-1 已不存在")));
        assert!(missing
            .warnings
            .iter()
            .any(|warning| warning.contains("prompt-1 已不存在")));
    }

    #[test]
    fn uncaptured_profiles_only_become_current() {
        let slot = CodexProjectSlot::default();
        let plan = compute_apply_plan(&slot, Some("provider-1"), true, &[], &[], None, false);
        assert_eq!(plan.provider_switch, None);
        assert!(plan.mcp_toggles.is_empty() && plan.skills_toggles.is_empty());
        assert_eq!(plan.prompt_activate, None);
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("尚未拍过")));
    }

    #[test]
    fn captured_empty_provider_and_prompt_never_switch_or_activate() {
        let slot = CodexProjectSlot {
            providers: Some(String::new()),
            mcp: None,
            skills: None,
            prompts: Some(String::new()),
        };
        let plan = compute_apply_plan(&slot, Some("provider-1"), true, &[], &[], None, true);
        assert!(slot.scope_captured(), "空串是拍过的标记，不是未拍");
        assert_eq!(plan.provider_switch, None, "拍到时无激活供应商则不切换");
        assert_eq!(plan.prompt_activate, None, "拍到时无激活指令则不激活");
        assert!(plan.warnings.is_empty());
    }

    #[test]
    fn apply_plan_switches_and_enables_exactly_the_captured_state() {
        let slot = CodexProjectSlot {
            providers: Some("provider-2".into()),
            mcp: Some(vec!["mcp-a".into()]),
            skills: None,
            prompts: Some("prompt-2".into()),
        };
        let plan = compute_apply_plan(
            &slot,
            Some("provider-1"),
            true,
            &[("mcp-a".into(), false)],
            &[],
            None,
            true,
        );
        assert_eq!(plan.provider_switch.as_deref(), Some("provider-2"));
        assert_eq!(plan.mcp_toggles, vec![("mcp-a".into(), true)]);
        assert_eq!(plan.prompt_activate.as_deref(), Some("prompt-2"));
        assert!(plan.warnings.is_empty());
    }
}
