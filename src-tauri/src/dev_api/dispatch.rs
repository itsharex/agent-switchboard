use super::http::{argument, as_json, optional_argument, InvokeRequest};
use crate::commands::{self, error::CommandError};
use crate::local_state::{AppSettings, CloudBackupSettings};
use asb_core::contracts::{
    AppKind, ModelUsageRequest, ProviderDraft, SettingsValues, UsageHistoryRequest,
};
use serde_json::Value;
use tauri::AppHandle;

pub(super) fn dispatch(app: &AppHandle, request: InvokeRequest) -> Result<Value, CommandError> {
    tauri::async_runtime::block_on(async {
        if let Some(result) = super::codex_dispatch::dispatch(app, &request).await? {
            return Ok(result);
        }
        if let Some(result) = super::claude_dispatch::dispatch(app, &request).await? {
            return Ok(result);
        }
        if let Some(result) = super::extensions_dispatch::dispatch(app, &request).await? {
            return Ok(result);
        }
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
            "pick_file" => command!(commands::window::pick_file(
                app.clone(),
                argument(&request.args, "filterName")?,
                argument(&request.args, "extensions")?,
            )),
            "config_status" => command!(commands::status::config_status(app.clone())),
            "runtime_overview" => command!(commands::status::runtime_overview(app.clone())),
            "preview_claude_gateway_stop" => command!(
                commands::switching::claude_gateway::preview_claude_gateway_stop(app.clone())
            ),
            "stop_claude_gateway" => {
                command!(commands::switching::claude_gateway::stop_claude_gateway(
                    app.clone(),
                    argument(&request.args, "preview")?,
                    argument(&request.args, "confirmWrite")?
                ))
            }
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
            "delete_profile" => command!(commands::delete_profile(
                app.clone(),
                argument(&request.args, "profileId")?,
                argument::<String>(&request.args, "expectedFileHash")?,
            )),
            "reorder_claude_profiles" => command!(commands::reorder_claude_profiles(
                app.clone(),
                argument(&request.args, "orderedIds")?,
                argument(&request.args, "expectedFileHashes")?,
            )),
            "import_discovered_claude_profile" => {
                command!(commands::import_discovered_claude_profile(app.clone()))
            }
            "scan_ccswitch" => command!(commands::scan_ccswitch(
                app.clone(),
                argument::<Option<String>>(&request.args, "dbDirectory")?,
            )),
            "import_ccswitch_claude_profiles" => {
                command!(commands::import_ccswitch_claude_profiles(
                    app.clone(),
                    argument(&request.args, "keys")?,
                    argument::<Option<String>>(&request.args, "dbDirectory")?,
                ))
            }
            "export_providers_sql" => command!(commands::export_providers_sql(
                app.clone(),
                argument(&request.args, "targetPath")?,
            )),
            "apply_providers_sql" => command!(commands::apply_providers_sql(
                app.clone(),
                argument(&request.args, "sqlPath")?,
            )),
            "import_providers_sql" => command!(commands::import_providers_sql(
                app.clone(),
                argument(&request.args, "ids")?,
                argument(&request.args, "sqlPath")?,
            )),
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
            "get_current_client_configuration" => {
                command!(commands::client_settings::get_current_client_configuration(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                ))
            }
            "parse_claude_extra_configuration" => {
                command!(commands::client_settings::parse_claude_extra_configuration(
                    argument::<String>(&request.args, "content")?,
                ))
            }
            "preview_client_configuration_apply" => {
                command!(commands::client_configuration_apply::preview_client_configuration_apply(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                    argument::<SettingsValues>(&request.args, "settings")?,
                    optional_argument::<asb_core::CodexSubagentSettings>(&request.args, "subagentSettings")?,
                ))
            }
            "preview_client_configuration_reset" => {
                command!(commands::client_configuration_apply::preview_client_configuration_reset(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                    argument::<commands::client_configuration_apply::ClientConfigurationResetKind>(&request.args, "resetKind")?,
                ))
            }
            "commit_client_configuration_apply" => {
                command!(commands::client_configuration_apply::commit_client_configuration_apply(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                    argument::<String>(&request.args, "expectedHash")?,
                    argument::<String>(&request.args, "expectedRenderedHash")?,
                    argument::<String>(&request.args, "expectedSettingsHash")?,
                    argument::<bool>(&request.args, "expectedTargetExisted")?,
                    argument::<SettingsValues>(&request.args, "settings")?,
                    optional_argument::<asb_core::CodexSubagentSettings>(&request.args, "subagentSettings")?,
                    argument::<bool>(&request.args, "confirmWrite")?,
                ))
            }
            "commit_client_configuration_reset" => {
                command!(commands::client_configuration_apply::commit_client_configuration_reset(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                    argument::<commands::client_configuration_apply::ClientConfigurationResetKind>(&request.args, "resetKind")?,
                    argument::<String>(&request.args, "expectedHash")?,
                    argument::<String>(&request.args, "expectedRenderedHash")?,
                    argument::<String>(&request.args, "expectedSettingsHash")?,
                    argument::<bool>(&request.args, "expectedTargetExisted")?,
                    argument::<bool>(&request.args, "confirmWrite")?,
                ))
            }
            "preview_manual_client_configuration" => {
                command!(commands::client_configuration_manual::preview_manual_client_configuration(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                    argument::<String>(&request.args, "expectedSourceHash")?,
                    argument::<String>(&request.args, "displayContent")?,
                    argument::<SettingsValues>(&request.args, "settings")?,
                    optional_argument::<asb_core::CodexSubagentSettings>(&request.args, "subagentSettings")?,
                ))
            }
            "commit_manual_client_configuration" => {
                command!(commands::client_configuration_manual::commit_manual_client_configuration(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                    argument::<String>(&request.args, "expectedSourceHash")?,
                    argument::<String>(&request.args, "expectedRenderedHash")?,
                    argument::<String>(&request.args, "expectedSettingsHash")?,
                    argument::<bool>(&request.args, "expectedTargetExisted")?,
                    argument::<String>(&request.args, "displayContent")?,
                    argument::<SettingsValues>(&request.args, "settings")?,
                    optional_argument::<asb_core::CodexSubagentSettings>(&request.args, "subagentSettings")?,
                    argument::<bool>(&request.args, "confirmWrite")?,
                ))
            }
            "preview_client_configuration_repair" => {
                command!(commands::client_configuration_repair::preview_client_configuration_repair(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                    argument::<String>(&request.args, "expectedSourceHash")?,
                ))
            }
            "commit_client_configuration_repair" => {
                command!(commands::client_configuration_repair::commit_client_configuration_repair(
                    app.clone(),
                    argument::<AppKind>(&request.args, "target")?,
                    argument::<String>(&request.args, "expectedSourceHash")?,
                    argument::<String>(&request.args, "expectedRenderedHash")?,
                    argument::<bool>(&request.args, "expectedTargetExisted")?,
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
                optional_argument(&request.args, "authHash")?,
                optional_argument(&request.args, "authExisted")?,
                optional_argument(&request.args, "authRenderedHash")?,
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
            "fetch_provider_models" => command!(commands::fetch_provider_models(
                app.clone(),
                argument(&request.args, "request",)?
            )),
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
            "delete_session" => command!(commands::delete_session(
                argument::<AppKind>(&request.args, "app")?,
                argument(&request.args, "sessionId")?,
            )),
            "delete_sessions" => command!(commands::delete_sessions(argument::<
                Vec<crate::session_manager::SessionDeleteRequest>,
            >(
                &request.args, "requests"
            )?)),
            _ => Err(CommandError::new(
                "web-command-unavailable",
                "浏览器开发环境不支持该原生窗口命令",
            )),
        }
    })
}
