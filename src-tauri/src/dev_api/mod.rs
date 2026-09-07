//! Browser-development bridge for the real local application backend.
//!
//! This module exists only in debug builds. The browser never gets a second
//! implementation of configuration behavior: every request dispatches to the
//! same typed Tauri command used by the desktop shell.

mod dispatch;
mod http;

#[cfg(all(test, target_os = "windows"))]
mod sandbox_tests;

#[cfg(test)]
mod extension_sandbox_tests;

#[cfg(test)]
mod tests;

pub(crate) const DEV_API_HOST: &str = "127.0.0.1";
pub(crate) const DEV_API_PORT: u16 = 1422;
pub(crate) const DEV_API_HEALTH_STATUS: u16 = 204;

fn dev_api_address() -> String {
    format!("{DEV_API_HOST}:{DEV_API_PORT}")
}

use http::handle_request;
use std::thread;
use tauri::AppHandle;
use tiny_http::Server;

/// Binds the loopback-only RPC bridge before the browser is opened.
pub(crate) fn start(app: AppHandle, development_origin: String) -> Result<(), String> {
    let server = Server::http(dev_api_address())
        .map_err(|error| format!("无法启动本机开发后端：{error}"))?;
    thread::Builder::new()
        .name("asb-web-dev-api".to_string())
        .spawn(move || serve(server, app, development_origin))
        .map_err(|error| format!("无法运行本机开发后端：{error}"))?;
    Ok(())
}

fn serve(server: Server, app: AppHandle, development_origin: String) {
    for mut request in server.incoming_requests() {
        let response = handle_request(&mut request, &app, &development_origin);
        let _ = request.respond(response);
    }
}
