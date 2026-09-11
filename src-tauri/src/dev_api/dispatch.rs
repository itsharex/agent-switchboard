use super::http::{argument, as_json, InvokeRequest};
use crate::commands::{self, error::CommandError};
use crate::local_state::{AppSettings, CloudBackupSettings};
use asb_core::contracts::{
    AppKind, CodexSubagentSettings, ModelUsageRequest, ProviderDraft, SettingsValues,
    SubagentSettingsPlan, UsageHistoryRequest,
};
use asb_core::extensions::contracts::ExtensionTarget;
use serde_json::Value;
use tauri::AppHandle;

pub(super) fn dispatch(app: &AppHandle, request: InvokeRequest) -> Result<Value, CommandError> {
    tauri::async_runtime::block_on(async {
        macro_rules! command {
            ($future:expr) => {
                as_json($future.await)
            };
        }

        match request.command.as_str() {
            "update_channel" => {
                as_json(Ok::<_, CommandError>(crate::distribution::update_channel()))
            }
            "pick_directory" => {
                command!(commands::window::pick_directory(app.clone()))
            }
            "config_status" => command!(commands::status::config_status(app.clone())),
            "runtime_overview" => command!(commands::status::runtime_overview(app.clone())),
            "gateway_status" => command!(commands::gateway::gateway_status(app.clone())),
            "gateway_retry_bind" => {
                command!(commands::gateway::gateway_retry_bind(app.clone()))
            }
            "gateway_prepare_port_change" => {
                command!(commands::gateway::gateway_prepare_port_change(
                    app.clone(),
                    argument(&request.args, "newPort")?,
                ))
            }
            "gateway_commit_port_change" => {
                command!(commands::gateway::gateway_commit_port_change(
                    app.clone(),
                    argument(&request.args, "preparationId")?,
                    argument(&request.args, "confirmWrite")?,
                ))
            }
            "gateway_cancel_port_change" => {
                command!(commands::gateway::gateway_cancel_port_change(
                    app.clone(),
                    argument(&request.args, "preparationId")?,
                ))
            }
            "gateway_discard_port_change" => {
                command!(commands::gateway::gateway_discard_port_change(
                    app.clone(),
                    argument(&request.args, "confirmWrite")?,
                ))
            }
            "list_profiles" => command!(commands::list_profiles(app.clone())),
            "list_codex_profiles" => command!(commands::list_codex_profiles(app.clone())),
            "create_codex_profile" => command!(commands::create_codex_profile(
                app.clone(),
                argument(&request.args, "draft")?,
            )),
            "delete_codex_profile" => command!(commands::delete_codex_profile(
                app.clone(),
                argument(&request.args, "profileId")?,
                argument(&request.args, "expectedFileHash")?,
            )),
            "reorder_codex_profiles" => command!(commands::reorder_codex_profiles(
                app.clone(),
                argument(&request.args, "orderedIds")?,
                argument(&request.args, "expectedFileHashes")?,
            )),
            "reset_profile_store" => command!(commands::reset_profile_store(
                app.clone(),
                argument(&request.args, "confirmWrite")?,
            )),
            "prepare_profile_save" => command!(commands::switching::prepare_profile_save(
                app.clone(),
                argument::<Option<String>>(&request.args, "profileId")?,
                argument::<ProviderDraft>(&request.args, "draft")?,
                argument::<Option<String>>(&request.args, "expectedFileHash")?,
            )),
            "commit_profile_save" => command!(commands::switching::commit_profile_save(
                app.clone(),
                argument::<String>(&request.args, "preparationId")?,
                argument::<bool>(&request.args, "confirmWrite")?,
            )),
            "prepare_codex_profile_save" => {
                command!(commands::switching::prepare_codex_profile_save(
                    app.clone(),
                    argument(&request.args, "profileId")?,
                    argument(&request.args, "draft")?,
                    argument(&request.args, "expectedFileHash")?,
                ))
            }
            "commit_codex_profile_save" => {
                command!(commands::switching::commit_codex_profile_save(
                    app.clone(),
                    argument(&request.args, "preparationId")?,
                    argument(&request.args, "confirmWrite")?,
                ))
            }
            "delete_profile" => command!(commands::delete_profile(
                app.clone(),
                argument(&request.args, "profileId")?,
                argument::<String>(&request.args, "expectedFileHash")?,
            )),
            "reorder_profiles" => command!(commands::reorder_profiles(
                app.clone(),
                argument::<AppKind>(&request.args, "target")?,
                argument(&request.args, "orderedIds")?,
                argument(&request.args, "expectedFileHashes")?,
            )),
            "import_discovered_claude_profile" => {
                command!(commands::import_discovered_claude_profile(app.clone()))
            }
            "scan_ccswitch" => command!(commands::scan_ccswitch(app.clone())),
            "import_ccswitch_claude_profiles" => {
                command!(commands::import_ccswitch_claude_profiles(
                    app.clone(),
                    argument(&request.args, "keys")?,
                ))
            }
            "get_provider_parameters_catalog" => as_json(Ok::<_, CommandError>(
                commands::client_settings::get_provider_parameters_catalog(argument::<AppKind>(
                    &request.args,
                    "target",
                )?),
            )),
            "get_client_settings_editor" => {
                command!(commands::client_settings::get_client_settings_editor(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                ))
            }
            "save_client_settings" => command!(commands::client_settings::save_client_settings(
                app.clone(),
                argument::<AppKind>(&request.args, "target")?,
                argument::<SettingsValues>(&request.args, "settings")?,
                argument::<String>(&request.args, "expectedSettingsHash")?,
            )),
            "preview_client_settings" => {
                command!(commands::client_settings::preview_client_settings(
                    argument::<AppKind>(&request.args, "target")?,
                    argument::<SettingsValues>(&request.args, "settings")?,
                ))
            }
            "get_codex_subagent_settings" => {
                command!(commands::subagent_settings::get_codex_subagent_settings(
                    app.clone(),
                ))
            }
            "preview_codex_subagent_settings_command" => {
                command!(
                    commands::subagent_settings::preview_codex_subagent_settings_command(
                        app.clone(),
                        argument::<CodexSubagentSettings>(&request.args, "settings")?,
                        argument::<String>(&request.args, "expectedHash")?,
                    )
                )
            }
            "apply_codex_subagent_settings" => {
                command!(commands::subagent_settings::apply_codex_subagent_settings(
                    app.clone(),
                    argument::<SubagentSettingsPlan>(&request.args, "plan")?,
                    argument::<bool>(&request.args, "confirmWrite")?,
                ))
            }
            "get_global_prompt_document" => {
                command!(commands::prompt_management::get_global_prompt_document(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                ))
            }
            "save_global_prompt_document" => {
                command!(commands::prompt_management::save_global_prompt_document(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                    argument(&request.args, "content")?,
                    argument(&request.args, "expectedHash")?,
                    argument(&request.args, "confirmWrite")?,
                ))
            }
            "get_app_settings" => command!(commands::get_app_settings(app.clone())),
            "set_app_settings" => command!(commands::set_app_settings(
                app.clone(),
                argument::<AppSettings>(&request.args, "settings")?,
            )),
            "get_cloud_backup_settings" => {
                command!(commands::cloud_backup::get_cloud_backup_settings(
                    app.clone()
                ))
            }
            "set_cloud_backup_settings" => {
                command!(commands::cloud_backup::set_cloud_backup_settings(
                    app.clone(),
                    argument::<CloudBackupSettings>(&request.args, "settings")?,
                ))
            }
            "cloud_backup_setup_sql" => {
                as_json(Ok(commands::cloud_backup::cloud_backup_setup_sql()))
            }
            "test_cloud_backup_connection" => {
                command!(commands::cloud_backup::test_cloud_backup_connection(
                    argument::<CloudBackupSettings>(&request.args, "settings")?,
                    argument(&request.args, "accountPassword")?,
                ))
            }
            "upload_cloud_backup" => command!(commands::cloud_backup::upload_cloud_backup(
                app.clone(),
                argument(&request.args, "accountPassword")?,
                argument(&request.args, "backupPassword")?,
                argument(&request.args, "confirmWrite")?,
            )),
            "restore_cloud_backup" => command!(commands::cloud_backup::restore_cloud_backup(
                app.clone(),
                argument(&request.args, "accountPassword")?,
                argument(&request.args, "backupPassword")?,
                argument(&request.args, "confirmWrite")?,
            )),
            "list_system_fonts" => command!(commands::list_system_fonts()),
            "preview_switch" => command!(commands::switching::preview_switch(
                app.clone(),
                argument(&request.args, "profileId")?,
            )),
            "execute_switch" => command!(commands::switching::execute_switch(
                app.clone(),
                argument(&request.args, "profileId")?,
                argument(&request.args, "expectedHash")?,
                argument(&request.args, "expectedRenderedHash")?,
                argument(&request.args, "confirmWrite")?,
            )),
            "list_backups" => command!(commands::switching::list_backups(app.clone())),
            "list_runtime_logs" => command!(commands::runtime_log::list_runtime_logs(app.clone())),
            "open_runtime_log_dir" => {
                command!(commands::runtime_log::open_runtime_log_dir(app.clone()))
            }
            "restore_backup" => command!(commands::switching::restore_backup(
                app.clone(),
                argument(&request.args, "backupId")?,
                argument(&request.args, "confirmWrite")?,
            )),
            "undo_last_switch" => command!(commands::switching::undo_last_switch(
                app.clone(),
                argument::<AppKind>(&request.args, "target")?,
                argument(&request.args, "confirmWrite")?,
            )),
            "backup_diff" => command!(commands::switching::backup_diff(
                app.clone(),
                argument(&request.args, "backupId")?,
            )),
            "open_backup_dir" => command!(commands::switching::open_backup_dir(app.clone())),
            "probe_endpoint" => {
                command!(commands::probe_endpoint(argument(&request.args, "url",)?))
            }
            "resolve_provider_endpoints" => as_json(commands::resolve_provider_endpoints(
                argument(&request.args, "request")?,
            )),
            "fetch_provider_models" => command!(commands::fetch_provider_models(argument(
                &request.args,
                "request",
            )?)),
            "prepare_provider_request" => {
                command!(commands::provider_request::prepare_provider_request(
                    app.clone(),
                    argument(&request.args, "target")?,
                ))
            }
            "fetch_provider_request_models" => {
                command!(commands::provider_request::fetch_provider_request_models(
                    app.clone(),
                    argument(&request.args, "requestId")?,
                ))
            }
            "execute_provider_request" => {
                command!(commands::provider_request::execute_provider_request(
                    app.clone(),
                    argument(&request.args, "request")?,
                ))
            }
            "cancel_provider_request" => {
                command!(commands::provider_request::cancel_provider_request(
                    app.clone(),
                    argument(&request.args, "requestId")?,
                ))
            }
            "test_usage_query" => command!(commands::test_usage_query(argument(
                &request.args,
                "request",
            )?)),
            "query_profile_usage" => command!(commands::query_profile_usage(
                app.clone(),
                argument(&request.args, "profileId")?,
            )),
            "read_profile_usage" => command!(commands::read_profile_usage(
                app.clone(),
                argument(&request.args, "profileId")?,
            )),
            "query_codex_official_quota" => command!(commands::query_codex_official_quota(
                app.clone(),
                argument(&request.args, "profileId")?,
            )),
            "get_cached_codex_official_reset" => {
                command!(commands::get_cached_codex_official_reset(app.clone()))
            }
            "refresh_codex_official_reset" => {
                command!(commands::refresh_codex_official_reset(app.clone()))
            }
            "official_login_start" => {
                command!(commands::official_login::official_login_start(argument::<
                    AppKind,
                >(
                    &request.args,
                    "target"
                )?,))
            }
            "official_login_poll" => {
                command!(commands::official_login::official_login_poll(argument::<
                    AppKind,
                >(
                    &request.args,
                    "target"
                )?,))
            }
            "official_login_cancel" => {
                command!(commands::official_login::official_login_cancel(argument::<
                    AppKind,
                >(
                    &request.args,
                    "target"
                )?,))
            }
            "get_cached_codex_reset_status" => {
                command!(commands::get_cached_codex_reset_status(app.clone()))
            }
            "check_codex_reset_status" => command!(commands::check_codex_reset_status(app.clone())),
            "lock_status" => command!(commands::status::lock_status(
                app.clone(),
                argument::<AppKind>(&request.args, "target")?,
            )),
            "recover_stale_lock" => command!(commands::status::recover_stale_lock(
                app.clone(),
                argument::<AppKind>(&request.args, "target")?,
            )),
            "discover_local" => command!(commands::discover_local(app.clone())),
            "discover_cached" => command!(commands::discover_cached(app.clone())),
            "get_model_usage_report" => {
                command!(commands::model_usage::get_model_usage_report(
                    app.clone(),
                    argument::<ModelUsageRequest>(&request.args, "request")?,
                ))
            }
            "get_usage_history" => command!(commands::usage_history::get_usage_history(
                app.clone(),
                argument::<UsageHistoryRequest>(&request.args, "request")?,
            )),
            "list_sessions" => command!(commands::list_sessions()),
            "get_session_messages" => command!(commands::get_session_messages(
                argument::<AppKind>(&request.args, "app")?,
                argument(&request.args, "sessionId")?,
            )),
            "resume_session" => command!(commands::resume_session(
                argument::<AppKind>(&request.args, "app")?,
                argument(&request.args, "sessionId")?,
            )),
            "list_extensions" => command!(commands::extensions::list_extensions(app.clone())),
            "recover_extension_transactions" => {
                command!(commands::extensions::recover_extension_transactions(
                    app.clone()
                ))
            }
            "discover_extensions" => {
                command!(commands::extensions::discover_extensions(app.clone()))
            }
            "save_extension" => command!(commands::extensions::save_extension(
                app.clone(),
                argument::<commands::extensions::ExtensionDraft>(&request.args, "draft")?,
            )),
            "get_mcp_edit_view" => command!(commands::extensions::get_mcp_edit_view(
                app.clone(),
                argument(&request.args, "definitionId")?,
            )),
            "update_mcp_definition" => command!(commands::extensions::update_mcp_definition(
                app.clone(),
                argument(&request.args, "definitionId")?,
                argument::<asb_core::extensions::McpEditRequest>(&request.args, "edit")?,
            )),
            "delete_extension" => command!(commands::extensions::delete_extension(
                app.clone(),
                argument(&request.args, "id")?,
                argument(&request.args, "confirmWrite")?,
            )),
            "register_project" => command!(commands::extensions::register_project(
                app.clone(),
                argument(&request.args, "root")?,
            )),
            "scan_local_skill_source" => command!(commands::extensions::scan_local_skill_source(
                app.clone(),
                argument(&request.args, "root")?,
            )),
            "resolve_skill_source" => command!(commands::extensions::resolve_skill_source(
                argument(&request.args, "repo")?,
                argument(&request.args, "subpath")?,
                argument::<Option<String>>(&request.args, "refName")?,
            )),
            "import_skill_candidate" => command!(commands::extensions::import_skill_candidate(
                app.clone(),
                argument(&request.args, "digest")?,
                argument(&request.args, "name")?,
                argument::<Option<AppKind>>(&request.args, "hostScoped")?,
            )),
            "import_discovered_skill" => command!(commands::extensions::import_discovered_skill(
                app.clone(),
                argument(&request.args, "observationId")?,
            )),
            "import_discovered_mcp" => command!(commands::extensions::import_discovered_mcp(
                app.clone(),
                argument(&request.args, "observationId")?,
            )),
            "preview_discovered_takeover" => {
                command!(commands::extensions::preview_discovered_takeover(
                    app.clone(),
                    argument(&request.args, "observationId")?,
                ))
            }
            "takeover_discovered_extension" => {
                command!(commands::extensions::takeover_discovered_extension(
                    app.clone(),
                    argument(&request.args, "observationId")?,
                    argument(&request.args, "confirmWrite")?,
                ))
            }
            "export_extension_portable" => {
                command!(commands::extensions::export_extension_portable(
                    app.clone(),
                    argument(&request.args, "definitionId")?,
                    argument(&request.args, "targetPath")?,
                ))
            }
            "import_extension_portable" => {
                command!(commands::extensions::import_extension_portable(
                    app.clone(),
                    argument(&request.args, "packagePath")?,
                    argument(&request.args, "confirmWrite")?,
                ))
            }
            "check_skill_updates" => command!(commands::extensions::check_skill_updates(
                app.clone(),
                argument(&request.args, "definitionIds")?,
            )),
            "update_skill_definition" => command!(commands::extensions::update_skill_definition(
                app.clone(),
                argument(&request.args, "definitionId")?,
                argument(&request.args, "newDigest")?,
            )),
            "create_local_skill" => command!(commands::extensions::create_local_skill(
                app.clone(),
                argument(&request.args, "draft")?,
            )),
            "fork_local_skill" => command!(commands::extensions::fork_local_skill(
                app.clone(),
                argument(&request.args, "definitionId")?,
            )),
            "get_skill_editor" => command!(commands::extensions::get_skill_editor(
                app.clone(),
                argument(&request.args, "definitionId")?,
            )),
            "update_skill_files" => command!(commands::extensions::update_skill_files(
                app.clone(),
                argument(&request.args, "definitionId")?,
                argument(&request.args, "update")?,
            )),
            "list_skill_versions" => command!(commands::extensions::list_skill_versions(
                app.clone(),
                argument(&request.args, "definitionId")?,
            )),
            "update_skill_dependencies" => {
                command!(commands::extensions::update_skill_dependencies(
                    app.clone(),
                    argument(&request.args, "definitionId")?,
                    argument(&request.args, "update")?,
                ))
            }
            "put_extension_secret" => command!(commands::extensions::put_extension_secret(
                argument(&request.args, "value")?,
                argument(&request.args, "purpose")?,
            )),
            "prepare_extension_plan" => command!(commands::extensions::prepare_extension_plan(
                app.clone(),
                argument::<commands::extensions::PlanRequest>(&request.args, "request")?,
            )),
            "prepare_extension_repair" => {
                command!(commands::extensions::prepare_extension_repair(
                    app.clone(),
                    argument::<commands::extensions::RepairRequest>(&request.args, "request")?,
                ))
            }
            "apply_extension_plan" => command!(commands::extensions::apply_extension_plan(
                app.clone(),
                argument(&request.args, "planId")?,
                argument(&request.args, "confirmWrite")?,
            )),
            "get_extension_operation" => command!(commands::extensions::get_extension_operation(
                app.clone(),
                argument(&request.args, "operationId")?,
            )),
            "prepare_extension_restore" => {
                command!(commands::extensions::prepare_extension_restore(
                    app.clone(),
                    argument(&request.args, "operationId")?,
                ))
            }
            "check_mcp_connection" => command!(commands::extensions::check_mcp_connection(
                app.clone(),
                argument(&request.args, "definitionId")?,
                argument::<ExtensionTarget>(&request.args, "target")?,
                argument(&request.args, "confirm")?,
            )),
            "get_mcp_check" => command!(commands::extensions::get_mcp_check(argument(
                &request.args,
                "checkId"
            )?,)),
            "cancel_mcp_check" => command!(commands::extensions::cancel_mcp_check(argument(
                &request.args,
                "checkId"
            )?,)),
            _ => Err(CommandError::new(
                "web-command-unavailable",
                "浏览器开发环境不支持该原生窗口命令",
            )),
        }
    })
}
