mod app_paths;
mod ccswitch_source;
mod cloud_backup;
mod codex_official_quota;
mod codex_reset;
mod commands;
mod config_store;
#[cfg(debug_assertions)]
mod dev_api;
mod distribution;
mod extensions;
mod fonts;
mod gateway;
mod local_state;
mod model_usage;
mod model_usage_cache;
mod official_login;
mod probe;
mod provider_diagnostics;
mod provider_request;
mod runtime_log;
mod session_manager;
#[cfg(test)]
mod test_client_paths;
mod tray;
mod usage_cache;
mod usage_history;
mod usage_query;

pub use commands::local_config_paths;

/// Hardware acceleration is a WebView2 (Windows) creation-time concept; other
/// platforms keep their webview engine defaults.
#[cfg(windows)]
const WRY_DEFAULT_WEBVIEW2_BROWSER_ARGS: &str =
    "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

#[cfg(debug_assertions)]
pub(crate) fn web_development_enabled(value: Option<&std::ffi::OsStr>) -> bool {
    value == Some(std::ffi::OsStr::new("1"))
}

#[cfg(windows)]
fn apply_hardware_acceleration(
    windows: &mut [tauri::utils::config::WindowConfig],
    hardware_acceleration: bool,
) {
    if hardware_acceleration {
        return;
    }

    for window in windows {
        // Supplying an explicit argument string replaces Wry's defaults, so
        // seed this exact default before adding the GPU switch.
        let arguments = window
            .additional_browser_args
            .get_or_insert_with(|| WRY_DEFAULT_WEBVIEW2_BROWSER_ARGS.to_string());
        if !arguments
            .split_ascii_whitespace()
            .any(|argument| argument == "--disable-gpu")
        {
            if !arguments.is_empty() {
                arguments.push(' ');
            }
            arguments.push_str("--disable-gpu");
        }
    }
}

#[cfg(windows)]
fn configure_hardware_acceleration<R: tauri::Runtime>(context: &mut tauri::Context<R>) {
    let identifier = context.config().identifier.clone();
    let hardware_acceleration = local_state::LocalState::from_identifier(&identifier)
        .and_then(|state| state.get_app_settings())
        // A missing or malformed app setting must not stop the recovery shell;
        // retain WebView2's current GPU-enabled default in that case.
        .map(|settings| settings.hardware_acceleration)
        .unwrap_or(true);

    apply_hardware_acceleration(&mut context.config_mut().app.windows, hardware_acceleration);
}

#[cfg(not(windows))]
fn configure_hardware_acceleration<R: tauri::Runtime>(_: &mut tauri::Context<R>) {}

/// Runs the Agent Switchboard desktop shell.
pub fn run() {
    use tauri::Manager;

    let mut context = tauri::generate_context!();
    let log_directory =
        app_paths::log_directory(&context.config().identifier).expect("无法定位应用日志目录");
    configure_hardware_acceleration(&mut context);
    #[cfg(windows)]
    let startup_windows = app_paths::prepare_windows(&mut context);

    tauri::Builder::default()
        // Registered first so a duplicate launch exits during plugin init,
        // before the gateway, tray, or any window state is created. The
        // callback runs in the surviving instance and restores its window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Err(error) = tray::tray_open_main(app.clone(), false) {
                log::warn!("重复启动已拦截，恢复主窗口失败: {error}");
            }
        }))
        .plugin(runtime_log::plugin(log_directory))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(move |app| {
            #[cfg(windows)]
            app_paths::build_windows(app.handle(), &startup_windows)
                .map_err(std::io::Error::other)?;
            let local =
                local_state::LocalState::from_app(app.handle()).map_err(std::io::Error::other)?;
            app.manage(commands::ConfigWriteGate::default());
            let write_gate = app.state::<commands::ConfigWriteGate>().inner().clone();
            // Startup can replay a pending port change before the controller
            // is published. Hold the same gate as every later client-config
            // transaction so that recovery has one write boundary.
            let _write_guard = write_gate.lock().map_err(std::io::Error::other)?;
            // Invalid or no-longer-convertible application configuration must
            // leave the shell alive so the existing reset flow can present a
            // deliberate recovery choice. The store remains unreadable to all
            // runtime commands until that recovery is confirmed.
            let configuration_ready = match local.initialize_configuration_schema() {
                Ok(()) => true,
                Err(error) => {
                    log::error!("供应商配置升级失败，已进入恢复状态: {error}");
                    false
                }
            };
            local
                .initialize_extension_schema()
                .map_err(std::io::Error::other)?;
            // The gateway controller always exists; a failed port bind or an
            // unusable state file becomes a visible runtime state instead of
            // refusing the window.
            app.manage(gateway::GatewayController::start(&local));
            app.manage(gateway::PortChangePreparations::default());
            app.manage(commands::switching::ProfileSavePreparations::default());
            app.manage(provider_request::ProviderRequests::default());
            if configuration_ready {
                commands::switching::recover_pending_profile_save(app.handle())
                    .map_err(std::io::Error::other)?;
            }
            // A malformed settings file is rejected by the typed settings
            // surface, but must never prevent the tray/window recovery shell
            // from starting. Default native window behavior remains usable.
            if let Ok(settings) = local_state::LocalState::from_app(app.handle())
                .and_then(|state| state.get_app_settings())
            {
                runtime_log::set_level(settings.runtime_log_level);
                let _ = commands::apply_desktop_settings(app.handle(), &settings);
            }
            if let Err(error) = tray::setup(app.handle()) {
                tray::recover_main(app.handle(), &error);
            }
            // The tray panel is a persistent surface; its usage data must
            // refresh even while the main window is hidden, closed to the
            // tray, or on another page. The scheduler thread owns that
            // cadence, so no renderer lifecycle can stop it.
            usage_query::scheduler::spawn(app.handle().clone());
            runtime_log::record_started();
            #[cfg(debug_assertions)]
            {
                let web_development = std::env::var_os("ASB_WEB_DEVELOPMENT");
                if web_development_enabled(web_development.as_deref()) {
                    let development_origin = app
                        .config()
                        .build
                        .dev_url
                        .as_ref()
                        .map(|url| url.origin().ascii_serialization())
                        .ok_or_else(|| std::io::Error::other("缺少浏览器开发地址"))?;
                    dev_api::start(app.handle().clone(), development_origin)
                        .map_err(std::io::Error::other)?;
                    // The persistent Vite process owns the one-shot browser launch;
                    // Tauri restarts this process for every backend hot reload.
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            tray::popup::window_event(window, event);
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window
                    .app_handle()
                    .try_state::<gateway::GatewayController>()
                    .map(|gateway| {
                        gateway.has_gateway_dependency(
                            &local_state::LocalState::from_app(window.app_handle())
                                .expect("startup resolved the state directory"),
                        )
                    })
                    .unwrap_or(false)
                {
                    api.prevent_close();
                    let _ = window.hide();
                    return;
                }
                if tray::should_absorb(window.app_handle()) {
                    api.prevent_close();
                    // A tray has already been built successfully, so hide is
                    // recoverable. Do not let a hide failure destroy the app.
                    let _ = window.hide();
                } else {
                    // The persistent hidden tray WebView is still a window;
                    // closing only main would otherwise leave the process alive.
                    api.prevent_close();
                    tray::request_explicit_exit();
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            tray::tray_snapshot,
            tray::tray_ready,
            tray::tray_resize,
            tray::tray_hide,
            tray::tray_open_main,
            tray::tray_switch,
            tray::tray_quit,
            commands::status::config_status,
            commands::status::runtime_overview,
            commands::gateway::gateway_status,
            commands::gateway::gateway_retry_bind,
            commands::gateway::gateway_prepare_port_change,
            commands::gateway::gateway_commit_port_change,
            commands::gateway::gateway_cancel_port_change,
            commands::gateway::gateway_discard_port_change,
            commands::list_profiles,
            commands::reset_profile_store,
            commands::switching::prepare_profile_save,
            commands::switching::commit_profile_save,
            commands::delete_profile,
            commands::reorder_profiles,
            commands::import_discovered_profile,
            commands::client_settings::get_provider_parameters_catalog,
            commands::client_settings::get_client_settings_editor,
            commands::client_settings::save_client_settings,
            commands::client_settings::preview_client_settings,
            commands::prompt_management::get_global_prompt_document,
            commands::prompt_management::save_global_prompt_document,
            commands::subagent_settings::get_codex_subagent_settings,
            commands::subagent_settings::preview_codex_subagent_settings_command,
            commands::subagent_settings::apply_codex_subagent_settings,
            commands::switching::preview_switch,
            commands::switching::execute_switch,
            commands::switching::list_backups,
            commands::runtime_log::list_runtime_logs,
            commands::runtime_log::open_runtime_log_dir,
            commands::switching::restore_backup,
            commands::switching::undo_last_switch,
            commands::switching::backup_diff,
            commands::switching::open_backup_dir,
            commands::cloud_backup::get_cloud_backup_settings,
            commands::cloud_backup::set_cloud_backup_settings,
            commands::cloud_backup::cloud_backup_setup_sql,
            commands::cloud_backup::test_cloud_backup_connection,
            commands::cloud_backup::upload_cloud_backup,
            commands::cloud_backup::restore_cloud_backup,
            commands::probe_endpoint,
            commands::resolve_provider_endpoints,
            commands::test_usage_query,
            commands::query_profile_usage,
            commands::read_profile_usage,
            commands::query_codex_official_quota,
            commands::get_cached_codex_official_reset,
            commands::refresh_codex_official_reset,
            commands::official_login::official_login_start,
            commands::official_login::official_login_poll,
            commands::official_login::official_login_cancel,
            commands::fetch_provider_models,
            commands::provider_request::prepare_provider_request,
            commands::provider_request::execute_provider_request,
            commands::provider_request::cancel_provider_request,
            commands::provider_request::fetch_provider_request_models,
            commands::get_cached_codex_reset_status,
            commands::check_codex_reset_status,
            commands::status::lock_status,
            commands::status::recover_stale_lock,
            commands::discover_local,
            commands::discover_cached,
            commands::model_usage::get_model_usage_report,
            commands::usage_history::get_usage_history,
            commands::list_sessions,
            commands::get_session_messages,
            commands::resume_session,
            commands::scan_ccswitch,
            commands::import_ccswitch_profiles,
            commands::window::window_minimize,
            commands::window::window_toggle_maximize,
            commands::window::window_is_maximized,
            commands::window::window_close,
            commands::window::restart_application,
            distribution::update_channel,
            commands::get_app_settings,
            commands::set_app_settings,
            commands::repair_app_settings,
            commands::list_system_fonts,
            commands::extensions::list_extensions,
            commands::extensions::recover_extension_transactions,
            commands::extensions::discover_extensions,
            commands::extensions::save_extension,
            commands::extensions::get_mcp_edit_view,
            commands::extensions::update_mcp_definition,
            commands::extensions::delete_extension,
            commands::extensions::set_binding_lock,
            commands::extensions::register_project,
            commands::extensions::scan_local_skill_source,
            commands::extensions::resolve_skill_source,
            commands::extensions::import_skill_candidate,
            commands::extensions::import_discovered_skill,
            commands::extensions::import_discovered_mcp,
            commands::extensions::preview_discovered_takeover,
            commands::extensions::takeover_discovered_extension,
            commands::extensions::export_extension_portable,
            commands::extensions::import_extension_portable,
            commands::extensions::check_skill_updates,
            commands::extensions::update_skill_definition,
            commands::extensions::create_local_skill,
            commands::extensions::fork_local_skill,
            commands::extensions::get_skill_editor,
            commands::extensions::update_skill_files,
            commands::extensions::list_skill_versions,
            commands::extensions::update_skill_dependencies,
            commands::extensions::put_extension_secret,
            commands::extensions::prepare_extension_plan,
            commands::extensions::prepare_extension_repair,
            commands::extensions::apply_extension_plan,
            commands::extensions::get_extension_operation,
            commands::extensions::prepare_extension_restore,
            commands::extensions::check_mcp_connection,
            commands::extensions::get_mcp_check,
            commands::extensions::cancel_mcp_check,
            #[cfg(debug_assertions)]
            commands::window::toggle_devtools,
        ])
        .build(context)
        .expect("Agent Switchboard 启动失败")
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { code, api, .. } = event {
                // A user-invoked desktop restart must never enter the
                // close-to-tray path. Tauri uses this dedicated code when it
                // relaunches the executable.
                if code == Some(tauri::RESTART_EXIT_CODE) {
                    return;
                }
                if app
                    .try_state::<gateway::GatewayController>()
                    .map(|gateway| {
                        gateway.has_gateway_dependency(
                            &local_state::LocalState::from_app(&app)
                                .expect("startup resolved the state directory"),
                        )
                    })
                    .unwrap_or(false)
                {
                    api.prevent_exit();
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                    return;
                }
                // Explicit quit and the configured close-to-exit action end
                // the process; implicit exits remain recoverable via the tray.
                if !tray::take_explicit_exit() && tray::should_absorb(app) {
                    api.prevent_exit();
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn disabling_hardware_acceleration_keeps_wry_defaults_and_adds_the_gpu_flag() {
        let mut windows = vec![tauri::utils::config::WindowConfig::default()];
        apply_hardware_acceleration(&mut windows, false);

        assert_eq!(
            windows[0].additional_browser_args.as_deref(),
            Some("--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-gpu")
        );
    }

    #[cfg(windows)]
    #[test]
    fn enabled_hardware_acceleration_leaves_existing_browser_arguments_unchanged() {
        let mut windows = vec![tauri::utils::config::WindowConfig::default()];
        windows[0].additional_browser_args =
            Some("--autoplay-policy=no-user-gesture-required".to_string());

        apply_hardware_acceleration(&mut windows, true);

        assert_eq!(
            windows[0].additional_browser_args.as_deref(),
            Some("--autoplay-policy=no-user-gesture-required")
        );
    }

    #[test]
    fn web_development_requires_the_exact_enabled_value() {
        assert!(web_development_enabled(Some(std::ffi::OsStr::new("1"))));
        assert!(!web_development_enabled(None));
        assert!(!web_development_enabled(Some(std::ffi::OsStr::new(""))));
        assert!(!web_development_enabled(Some(std::ffi::OsStr::new("0"))));
        assert!(!web_development_enabled(Some(std::ffi::OsStr::new("true"))));
    }
}
