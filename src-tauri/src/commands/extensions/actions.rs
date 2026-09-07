//! Per-observation action eligibility: the backend judges what each
//! discovered row may do — copy into the library, be taken over (managed),
//! or whether an equivalent definition already exists — so the renderer
//! never offers an action that could only fail after the click. Judgments
//! are content-based (skill digest, fully-parsed MCP candidate), never
//! name-only.

use std::fs;
use std::path::Path;

use asb_core::contracts::AppKind;
use asb_core::extensions::contracts::{
    ExtensionKind, ExtensionPayload, McpDefinition, ObservedExtension, ObservedOrigin,
};
use asb_core::extensions::mcp::{
    import_claude_project_private_server, import_claude_server, import_codex_server,
};
use serde::Serialize;

use crate::extensions::discovery::DiscoveredPaths;
use crate::extensions::store::ExtensionStore;

/// What the renderer may offer for one discovered row.
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct ObservationActionsDto {
    /// Copy the content into the library; the original stays independent.
    import: ActionSupportDto,
    /// Start managing the current location as-is, recording the original
    /// state for a later restore.
    takeover: ActionSupportDto,
    /// The library definition that manages this row, when managed.
    managed_definition_id: Option<String>,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub(super) struct ActionSupportDto {
    supported: bool,
    /// An equivalent definition already exists in the library.
    in_library: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

fn unsupported(reason: impl Into<String>) -> ActionSupportDto {
    ActionSupportDto {
        supported: false,
        in_library: false,
        reason: Some(reason.into()),
    }
}

fn supported() -> ActionSupportDto {
    ActionSupportDto {
        supported: true,
        in_library: false,
        reason: None,
    }
}

/// Judges the actions of one observation against the current library and
/// disk. Advisory by design: the import and takeover commands re-verify
/// everything (digests, document digests, registration) at action time.
pub(super) fn observation_actions(
    store: &ExtensionStore,
    observed: &ObservedExtension,
    paths: &DiscoveredPaths,
) -> ObservationActionsDto {
    let bindings = store.list_bindings().unwrap_or_default();
    let managed_definition_id = observed.managed_binding_ids.first().and_then(|binding_id| {
        bindings
            .iter()
            .find(|binding| &binding.id == binding_id)
            .map(|binding| binding.resource_id.clone())
    });
    let definitions = store.list_definitions().unwrap_or_default();
    let read_only_origin = matches!(
        observed.origin,
        ObservedOrigin::LegacyRoot { .. } | ObservedOrigin::Managed { .. }
    );
    let (import, takeover) = match observed.kind {
        ExtensionKind::Skill => match observed.content_digest.as_deref() {
            None => {
                let reason = "该目录没有可复制的内容（缺少 SKILL.md 或清单无效）";
                (unsupported(reason), unsupported(reason))
            }
            Some(digest) => {
                let in_library = definitions.iter().any(|definition| {
                    matches!(
                        &definition.payload,
                        ExtensionPayload::Skill(skill) if skill.content_digest == digest
                    )
                });
                let mut import = supported();
                import.in_library = in_library;
                let takeover = if read_only_origin {
                    unsupported("只读来源不能纳入管理")
                } else {
                    supported()
                };
                (import, takeover)
            }
        },
        ExtensionKind::Mcp => match mcp_candidate(observed, paths) {
            None => (
                unsupported("该条目包含本应用未建模的配置，无法复制入库"),
                unsupported("该条目无法完整表达，不能纳入管理"),
            ),
            Some(candidate) => {
                let in_library = definitions.iter().any(|definition| {
                    definition.name == observed.name
                        && definition.payload == ExtensionPayload::Mcp(candidate.clone())
                });
                let mut import = supported();
                import.in_library = in_library;
                let takeover = if read_only_origin {
                    unsupported("只读来源不能纳入管理")
                } else if let ObservedOrigin::ProjectRoot { project_path } = &observed.origin {
                    let registered = store
                        .list_projects()
                        .unwrap_or_default()
                        .iter()
                        .any(|project| &project.root == project_path);
                    if registered {
                        supported()
                    } else {
                        unsupported("项目目录尚未注册；请先注册项目")
                    }
                } else {
                    supported()
                };
                (import, takeover)
            }
        },
    };
    ObservationActionsDto {
        import,
        takeover,
        managed_definition_id,
    }
}

/// Re-parses the observed native MCP entry into the library definition it
/// would become, or `None` when the entry cannot be fully expressed.
fn mcp_candidate(observed: &ObservedExtension, paths: &DiscoveredPaths) -> Option<McpDefinition> {
    let document = fs::read_to_string(Path::new(&observed.path)).ok()?;
    let claude_user_document =
        crate::extensions::paths::claude_user_json_path(&paths.home, paths.claude_dir.as_deref());
    let is_claude_private = matches!(
        (&observed.client, &observed.origin),
        (AppKind::Claude, ObservedOrigin::ProjectRoot { .. })
    ) && Path::new(&observed.path) == claude_user_document;
    match (&observed.client, &observed.origin) {
        (AppKind::Claude, ObservedOrigin::ProjectRoot { project_path }) if is_claude_private => {
            import_claude_project_private_server(&document, project_path, &observed.name).ok()
        }
        (AppKind::Codex, _) => import_codex_server(&document, &observed.name).ok(),
        (AppKind::Claude, _) => import_claude_server(&document, &observed.name).ok(),
    }
}
