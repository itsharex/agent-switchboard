//! MCP document targeting: the one owner of client-and-scope-to-path,
//! syntax and patch surface, plus the document read/hash/restore
//! helpers shared by every planning domain.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    DocumentSyntax, ExtensionBinding, ExtensionTarget, ManagedBaseline, ManagedBaselineFile,
};
use asb_core::extensions::mcp::apply_claude_entry_restore;
use asb_core::extensions::plan::{ExtensionPlan, PlanStep, PlannedOperation};

use crate::commands::error::CommandError;
use crate::commands::extensions::support::*;
use crate::extensions::store::ExtensionStore;

use super::{McpDocumentTarget, McpScope, Planner};

impl Planner<'_> {
    /// in `<project>/.codex/config.toml` (TOML, `mcp_servers`); Claude
    /// project-shared in `<project>/.mcp.json`. Codex has no private
    /// project document — that combination is rejected here, before any
    /// path or patch exists.
    pub(super) fn mcp_document(
        &self,
        binding: &ExtensionBinding,
    ) -> Result<McpDocumentTarget, CommandError> {
        match &binding.target {
            ExtensionTarget::App { client } => Ok(match client {
                AppKind::Codex => McpDocumentTarget {
                    path: crate::extensions::paths::codex_config_path(
                        &self.paths.home,
                        self.paths.codex_home.as_deref(),
                    ),
                    syntax: DocumentSyntax::Toml,
                    scope: McpScope::CodexServers,
                },
                AppKind::Claude => McpDocumentTarget {
                    path: crate::extensions::paths::claude_user_json_path(
                        &self.paths.home,
                        self.paths.claude_dir.as_deref(),
                    ),
                    syntax: DocumentSyntax::Json,
                    scope: McpScope::ClaudeUserServers,
                },
            }),
            ExtensionTarget::ProjectShared { client, .. } => {
                let root = self.project_path(binding)?;
                Ok(match client {
                    AppKind::Codex => McpDocumentTarget {
                        path: crate::extensions::paths::codex_project_config_path(
                            std::path::Path::new(&root),
                        ),
                        syntax: DocumentSyntax::Toml,
                        scope: McpScope::CodexServers,
                    },
                    AppKind::Claude => McpDocumentTarget {
                        path: crate::extensions::paths::claude_project_mcp_path(
                            std::path::Path::new(&root),
                        ),
                        syntax: DocumentSyntax::Json,
                        scope: McpScope::ClaudeUserServers,
                    },
                })
            }
            ExtensionTarget::ProjectPrivate { client, .. } => match client {
                AppKind::Claude => {
                    let root = self.project_path(binding)?;
                    Ok(McpDocumentTarget {
                        path: crate::extensions::paths::claude_user_json_path(
                            &self.paths.home,
                            self.paths.claude_dir.as_deref(),
                        ),
                        syntax: DocumentSyntax::Json,
                        scope: McpScope::ClaudeProjectPrivate { project_path: root },
                    })
                }
                AppKind::Codex => Err(CommandError::new(
                    "extension-unsupported-target",
                    "Codex 没有项目私有配置文件，不能建立项目私有 MCP 绑定",
                )),
            },
        }
    }
}

pub(super) fn read_document(path: &std::path::Path) -> Result<Option<String>, CommandError> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(CommandError::new(
            "extension-external-change",
            format!("无法读取 {}；请先解决外部文件状态", path.display()),
        )),
    }
}

pub(super) fn document_hash_of(path: &std::path::Path) -> Result<Option<String>, CommandError> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(sha_hex(&bytes))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(CommandError::new(
            "extension-external-change",
            format!("无法读取 {}；请先解决外部文件状态", path.display()),
        )),
    }
}

// ---------------------------------------------------------------- batch documents

/// Working state of one client document across a batch plan. Every entry
/// patch of the batch threads through `rendered`, so one document yields
/// exactly one materialized `DocumentWrite` no matter how many operations
/// touch it — the same threading the repair path applies.
pub(super) struct DocumentWork {
    pub(super) client: AppKind,
    pub(super) syntax: DocumentSyntax,
    /// Digest of the document as it sat on disk when the batch first
    /// touched it; the materialized write's expectation.
    pub(super) expected_content_hash: Option<String>,
    /// Whether the document existed at first touch.
    pub(super) expected_existed: bool,
    /// The batch's progressively patched text.
    pub(super) rendered: String,
    /// Flat target index (batch order) of the target that carries the
    /// materialized write: the first target that actually changed the text.
    pub(super) writer: Option<usize>,
    /// Entry positions already claimed by another binding of this batch.
    pub(super) claimed: BTreeSet<String>,
}

/// The batch-scoped document states plus the flat target counter the
/// planner advances as it pushes targets.
#[derive(Default)]
pub(crate) struct DocumentBatch {
    documents: BTreeMap<String, DocumentWork>,
    targets: usize,
}

impl DocumentBatch {
    /// The flat index of the next target the current builder will push.
    pub(super) fn targets(&self) -> usize {
        self.targets
    }

    pub(super) fn advance(&mut self, count: usize) {
        self.targets += count;
    }

    pub(super) fn documents_mut(&mut self) -> &mut BTreeMap<String, DocumentWork> {
        &mut self.documents
    }
}

impl DocumentWork {
    /// Claims one entry position for the target being built. Two bindings
    /// of one batch may never write the same position; the first changing
    /// target of a document becomes its writer.
    pub(super) fn claim(&mut self, pointer: &str, writer: usize) -> Result<(), CommandError> {
        if !self.claimed.insert(pointer.to_string()) {
            return Err(CommandError::new(
                "extension-conflict",
                format!("条目 {pointer} 已被同一批量计划中的另一绑定使用"),
            ));
        }
        if self.writer.is_none() {
            self.writer = Some(writer);
        }
        Ok(())
    }
}

impl Planner<'_> {
    /// The batch's working state for one document, reading the file on
    /// first touch. Every patch of the batch applies to this shared text.
    pub(super) fn document_work<'a>(
        &self,
        batch: &'a mut DocumentBatch,
        document: &std::path::Path,
        client: AppKind,
        syntax: DocumentSyntax,
    ) -> Result<&'a mut DocumentWork, CommandError> {
        let key = document.to_string_lossy().to_string();
        if !batch.documents.contains_key(&key) {
            let current = read_document(document)?;
            let empty = match client {
                AppKind::Codex => String::new(),
                AppKind::Claude => "{}".to_string(),
            };
            batch.documents.insert(
                key.clone(),
                DocumentWork {
                    client,
                    syntax,
                    expected_content_hash: document_hash_of(document)?,
                    expected_existed: current.is_some(),
                    rendered: current.unwrap_or(empty),
                    writer: None,
                    claimed: BTreeSet::new(),
                },
            );
        }
        Ok(batch.documents.get_mut(&key).expect("just inserted"))
    }
}

/// Folds the batch's threaded document states into the plan: one
/// `DocumentWrite` per touched document, carried by the first target that
/// changed it, and every planned baseline of that document moves to the
/// final rendered hash so the committed post-state matches the disk.
pub(super) fn materialize_document_writes(
    batch: &mut DocumentBatch,
    operations: &mut [PlannedOperation],
) {
    let mut flat: Vec<(usize, usize)> = Vec::new();
    for (operation_index, operation) in operations.iter().enumerate() {
        for target_index in 0..operation.targets.len() {
            flat.push((operation_index, target_index));
        }
    }
    let documents = batch.documents_mut();
    for (path, work) in documents.iter_mut() {
        let Some(writer) = work.writer else {
            continue;
        };
        let final_hash = sha_hex(work.rendered.as_bytes());
        let Some(&(operation_index, target_index)) = flat.get(writer) else {
            continue;
        };
        operations[operation_index].targets[target_index]
            .steps
            .push(PlanStep::DocumentWrite {
                client: work.client,
                path: path.clone(),
                expected_content_hash: work.expected_content_hash.clone(),
                expected_existed: work.expected_existed,
                rendered: work.rendered.clone(),
                syntax: work.syntax,
                backup_dir: String::new(),
            });
        for target in operations
            .iter_mut()
            .flat_map(|operation| operation.targets.iter_mut())
        {
            let Some(baseline) = &mut target.baseline else {
                continue;
            };
            for entry in &mut baseline.entries {
                match entry {
                    ManagedBaseline::DocumentEntry {
                        target_path,
                        last_document_hash,
                        ..
                    }
                    | ManagedBaseline::SetMember {
                        target_path,
                        last_document_hash,
                        ..
                    } if *target_path == *path => {
                        *last_document_hash = final_hash.clone();
                    }
                    ManagedBaseline::DocumentEntry { .. }
                    | ManagedBaseline::SetMember { .. }
                    | ManagedBaseline::Directory { .. } => {}
                }
            }
        }
    }
}

/// Whether two MCP document scopes address the same native namespace: the
/// same document alone is not enough — Claude user-level and one project's
/// private section share one file but never one key space.
pub(super) fn same_mcp_namespace(a: &McpScope, b: &McpScope) -> bool {
    match (a, b) {
        (McpScope::CodexServers, McpScope::CodexServers)
        | (McpScope::ClaudeUserServers, McpScope::ClaudeUserServers) => true,
        (
            McpScope::ClaudeProjectPrivate { project_path: left },
            McpScope::ClaudeProjectPrivate {
                project_path: right,
            },
        ) => left == right,
        _ => false,
    }
}

/// Restores one adopted Claude entry verbatim from its takeover baseline.
/// The stored original is the canonical JSON text of the entry as adopted.
pub(super) fn restore_claude_entry(
    text: &str,
    pointer_path: &[&str],
    key: &str,
    original: &str,
    pointer: &str,
) -> Result<(String, Vec<asb_core::extensions::mcp::EntryChange>), CommandError> {
    let original_json: serde_json::Value = serde_json::from_str(original).map_err(|_| {
        CommandError::new(
            "extension-baseline",
            "记录的原始 MCP 条目无法解析，不能恢复接管内容",
        )
    })?;
    let (rendered, mut changes) =
        apply_claude_entry_restore(text, pointer_path, key, Some(&original_json))
            .map_err(adapter_error)?;
    if changes.is_empty() {
        changes.push(asb_core::extensions::mcp::EntryChange {
            pointer: pointer.to_string(),
            before: None,
            after: Some(original.to_string()),
        });
    }
    Ok((rendered, changes))
}

/// One document write changes the whole-file digest for every binding that
/// owns an entry in that document. Keep those baselines in the same library
/// commit as the write; otherwise a later operation on a different managed
/// entry would mistake this application's own change for an external edit.
pub(super) fn synchronize_document_baselines(
    store: &ExtensionStore,
    plan: &ExtensionPlan,
    removed_bindings: &BTreeSet<String>,
) -> Result<Vec<(String, ManagedBaselineFile)>, CommandError> {
    let mut hashes = BTreeMap::new();
    let mut repaired_documents = BTreeSet::new();
    for operation in &plan.operations {
        if operation.operation != asb_core::extensions::contracts::PlanOperation::Repair {
            continue;
        }
        for step in operation
            .targets
            .iter()
            .flat_map(|target| target.steps.iter())
        {
            if let PlanStep::DocumentWrite { path, .. } = step {
                repaired_documents.insert(path.clone());
            }
        }
    }
    for step in plan
        .planned_targets()
        .flat_map(|target| target.steps.iter())
    {
        let PlanStep::DocumentWrite {
            path,
            expected_content_hash,
            rendered,
            ..
        } = step
        else {
            continue;
        };
        let next = (expected_content_hash.clone(), sha_hex(rendered.as_bytes()));
        if let Some(existing) = hashes.insert(path.clone(), next.clone()) {
            if existing != next {
                return Err(CommandError::new(
                    "extension-invalid",
                    format!(
                        "同一计划为 {} 生成了相互冲突的文档内容",
                        std::path::Path::new(path).display()
                    ),
                ));
            }
        }
    }
    let mut baselines: BTreeMap<String, ManagedBaselineFile> = plan
        .planned_baselines()
        .into_iter()
        .filter(|(binding_id, _)| !removed_bindings.contains(binding_id))
        .map(|(binding_id, baseline)| (binding_id, baseline.clone()))
        .collect();

    for binding in store.list_bindings().map_err(store_error)? {
        if removed_bindings.contains(&binding.id) || baselines.contains_key(&binding.id) {
            continue;
        }
        let Some(mut baseline) = store.get_baseline_file(&binding.id).map_err(store_error)? else {
            continue;
        };
        if update_baseline_document_hashes(&mut baseline, &hashes)
            || update_repaired_document_hashes(&mut baseline, &hashes, &repaired_documents)
        {
            baselines.insert(binding.id, baseline);
        }
    }
    for baseline in baselines.values_mut() {
        update_baseline_document_hashes(baseline, &hashes);
        update_repaired_document_hashes(baseline, &hashes, &repaired_documents);
    }
    Ok(baselines.into_iter().collect())
}

/// Repair explicitly accepts a missing owned entry whose restoration was
/// re-validated while building the plan. That deletion necessarily made each
/// peer binding's whole-document hash stale, so all peers in the repaired
/// document move to the verified post-write hash together.
fn update_repaired_document_hashes(
    baseline: &mut ManagedBaselineFile,
    hashes: &BTreeMap<String, (Option<String>, String)>,
    repaired_documents: &BTreeSet<String>,
) -> bool {
    let mut changed = false;
    for entry in &mut baseline.entries {
        let (target_path, last_document_hash) = match entry {
            ManagedBaseline::DocumentEntry {
                target_path,
                last_document_hash,
                ..
            }
            | ManagedBaseline::SetMember {
                target_path,
                last_document_hash,
                ..
            } => (target_path, last_document_hash),
            ManagedBaseline::Directory { .. } => continue,
        };
        if !repaired_documents.contains(target_path) {
            continue;
        }
        let Some((_, next_hash)) = hashes.get(target_path) else {
            continue;
        };
        if last_document_hash != next_hash {
            *last_document_hash = next_hash.clone();
            changed = true;
        }
    }
    changed
}

pub(super) fn update_baseline_document_hashes(
    baseline: &mut ManagedBaselineFile,
    hashes: &BTreeMap<String, (Option<String>, String)>,
) -> bool {
    let mut changed = false;
    for entry in &mut baseline.entries {
        let (target_path, last_document_hash) = match entry {
            ManagedBaseline::DocumentEntry {
                target_path,
                last_document_hash,
                ..
            }
            | ManagedBaseline::SetMember {
                target_path,
                last_document_hash,
                ..
            } => (target_path, last_document_hash),
            ManagedBaseline::Directory { .. } => continue,
        };
        let Some((expected_hash, next_hash)) = hashes.get(target_path) else {
            continue;
        };
        // Only baselines that described the document at plan time may move
        // forward with this write. A stale baseline remains stale rather
        // than being silently blessed because another entry was changed.
        if expected_hash.as_ref() != Some(last_document_hash) {
            continue;
        }
        if last_document_hash != next_hash {
            *last_document_hash = next_hash.clone();
            changed = true;
        }
    }
    changed
}
