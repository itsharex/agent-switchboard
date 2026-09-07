//! Tauri commands for the extensions workspace. Commands orchestrate the
//! extension store, discovery, sources, secrets, and checks; every real
//! client-file write goes through `asb_switch::extensions`, and every plan
//! must be prepared, previewed, and confirmed before it can be applied.

mod apply;
mod checks;
mod definitions;
mod planner;
mod portable;
mod skills;
mod sources;
mod support;
mod takeover;
mod workspace;

pub use apply::{
    __cmd__apply_extension_plan, __cmd__get_extension_operation, __cmd__prepare_extension_restore,
    __tauri_command_name_apply_extension_plan, __tauri_command_name_get_extension_operation,
    __tauri_command_name_prepare_extension_restore, apply_extension_plan, get_extension_operation,
    prepare_extension_restore,
};
pub use checks::{
    __cmd__cancel_mcp_check, __cmd__check_mcp_connection, __cmd__get_mcp_check,
    __tauri_command_name_cancel_mcp_check, __tauri_command_name_check_mcp_connection,
    __tauri_command_name_get_mcp_check, cancel_mcp_check, check_mcp_connection, get_mcp_check,
};
#[cfg(debug_assertions)]
pub use definitions::ExtensionDraft;
pub use definitions::{
    __cmd__delete_extension, __cmd__get_mcp_edit_view, __cmd__put_extension_secret,
    __cmd__register_project, __cmd__save_extension, __cmd__set_binding_lock,
    __cmd__update_mcp_definition, __tauri_command_name_delete_extension,
    __tauri_command_name_get_mcp_edit_view, __tauri_command_name_put_extension_secret,
    __tauri_command_name_register_project, __tauri_command_name_save_extension,
    __tauri_command_name_set_binding_lock, __tauri_command_name_update_mcp_definition,
    delete_extension, get_mcp_edit_view, put_extension_secret, register_project, save_extension,
    set_binding_lock, update_mcp_definition,
};
#[cfg(debug_assertions)]
pub use planner::PlanRequest;
pub use planner::{
    __cmd__prepare_extension_plan, __tauri_command_name_prepare_extension_plan,
    prepare_extension_plan,
};
pub use portable::{
    __cmd__export_extension_portable, __cmd__import_extension_portable,
    __tauri_command_name_export_extension_portable, __tauri_command_name_import_extension_portable,
    export_extension_portable, import_extension_portable,
};
pub use skills::{
    __cmd__check_skill_updates, __cmd__create_local_skill, __cmd__fork_local_skill,
    __cmd__get_skill_editor, __cmd__list_skill_versions, __cmd__update_skill_definition,
    __cmd__update_skill_dependencies, __cmd__update_skill_files,
    __tauri_command_name_check_skill_updates, __tauri_command_name_create_local_skill,
    __tauri_command_name_fork_local_skill, __tauri_command_name_get_skill_editor,
    __tauri_command_name_list_skill_versions, __tauri_command_name_update_skill_definition,
    __tauri_command_name_update_skill_dependencies, __tauri_command_name_update_skill_files,
    check_skill_updates, create_local_skill, fork_local_skill, get_skill_editor,
    list_skill_versions, update_skill_definition, update_skill_dependencies, update_skill_files,
};
pub use sources::{
    __cmd__import_discovered_mcp, __cmd__import_discovered_skill, __cmd__import_skill_candidate,
    __cmd__resolve_skill_source, __cmd__scan_local_skill_source,
    __tauri_command_name_import_discovered_mcp, __tauri_command_name_import_discovered_skill,
    __tauri_command_name_import_skill_candidate, __tauri_command_name_resolve_skill_source,
    __tauri_command_name_scan_local_skill_source, import_discovered_mcp, import_discovered_skill,
    import_skill_candidate, resolve_skill_source, scan_local_skill_source,
};
pub use takeover::{
    __cmd__preview_discovered_takeover, __cmd__takeover_discovered_extension,
    __tauri_command_name_preview_discovered_takeover,
    __tauri_command_name_takeover_discovered_extension, preview_discovered_takeover,
    takeover_discovered_extension,
};
pub use workspace::{
    __cmd__discover_extensions, __cmd__list_extensions, __cmd__recover_extension_transactions,
    __tauri_command_name_discover_extensions, __tauri_command_name_list_extensions,
    __tauri_command_name_recover_extension_transactions, discover_extensions, list_extensions,
    recover_extension_transactions,
};
