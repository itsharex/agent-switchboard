mod app_paths;
mod ccswitch_source;
mod claude_auth;
mod claude_env_conflicts;
mod claude_integration;
mod claude_mcp_source;
mod claude_native_quota;
mod claude_prompts;
mod claude_session_usage;
mod claude_snippets;
mod cloud_backup;
mod codex_auth;
mod codex_common;
mod codex_env_conflicts;
mod codex_metering;
mod codex_official_quota;
mod codex_prompts;
mod codex_reset;
mod command_registry;
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
mod outbound_proxy;
mod probe;
mod provider_diagnostics;
mod provider_request;
mod runtime_log;
mod session_manager;
#[cfg(test)]
mod test_client_paths;
mod tray;
mod upstream_overrides;
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
        .plugin(tauri_plugin_dialog::init())
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
            // Outbound proxy settings gate every gateway-side HTTP client;
            // load them before any client is built. A malformed file defaults
            // to direct egress instead of blocking startup.
            if let Err(error) = outbound_proxy::load(local.root()) {
                log::warn!("出站代理设置不可用，按直连处理：{error}");
            }
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
            app.manage(gateway::GatewayController::start_with_write_lock(
                &local,
                write_gate.shared(),
            ));
            app.manage(gateway::PortChangePreparations::default());
            app.manage(commands::switching::ProfileSavePreparations::default());
            app.manage(commands::switching::CodexProfileSavePreparations::default());
            app.manage(commands::switching::CodexPolicyPreparations::default());
            app.manage(provider_request::ProviderRequests::default());
            if configuration_ready {
                commands::switching::recover_pending_profile_save(app.handle())
                    .map_err(std::io::Error::other)?;
            }
            if let Err(error) = commands::switching::codex_policy::recover_on_startup(
                &local, app.state::<gateway::GatewayController>().inner()) {
                log::error!("Codex 网关策略需要恢复，已保留原始事务：{error}");
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
        .invoke_handler(crate::command_registry::handler!())
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
                // Explicit quit and the configured close-to-exit action end
                // the process; implicit exits remain recoverable via the tray.
                if !tray::take_explicit_exit() && tray::should_absorb(app) {
                    api.prevent_exit();
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.hide();
                    }
                } else if let Some(gateway) = app.try_state::<gateway::GatewayController>() {
                    if let Err(error) = commands::switching::claude_gateway::restore_on_exit(app) {
                        api.prevent_exit();
                        log::error!("Claude 接管恢复失败，已取消退出：{error}");
                        tray::recover_main(app, "Claude 接管恢复失败");
                        use tauri_plugin_dialog::DialogExt;
                        app.dialog().message(format!("Claude 接管恢复失败，已取消退出。{error}\n请在切换历史中处理恢复后再退出。"))
                            .title("Agent Switchboard")
                            .kind(tauri_plugin_dialog::MessageDialogKind::Error).show(|_| {});
                    } else {
                        gateway.shutdown();
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
