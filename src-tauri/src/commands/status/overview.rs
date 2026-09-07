use serde::Serialize;
use std::path::Path;

/// Stable, non-secret facts about the running Agent Switchboard process.
/// This is deliberately separate from client configuration status: it owns
/// only application metadata and never reads Codex or Claude Code files.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeOverview {
    pub app_version: String,
    pub build_mode: RuntimeBuildMode,
    pub platform: String,
    pub architecture: String,
    pub transport: RuntimeTransport,
    pub app_data_path: String,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeBuildMode {
    Debug,
    Release,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RuntimeTransport {
    DesktopProtocol,
    #[cfg(debug_assertions)]
    WebDevelopment {
        host: String,
        port: u16,
        #[serde(rename = "healthStatus")]
        health_status: u16,
    },
}

pub(super) fn runtime_transport() -> RuntimeTransport {
    #[cfg(debug_assertions)]
    {
        let web_development = std::env::var_os("ASB_WEB_DEVELOPMENT");
        if crate::web_development_enabled(web_development.as_deref()) {
            return RuntimeTransport::WebDevelopment {
                host: crate::dev_api::DEV_API_HOST.to_string(),
                port: crate::dev_api::DEV_API_PORT,
                health_status: crate::dev_api::DEV_API_HEALTH_STATUS,
            };
        }
    }

    RuntimeTransport::DesktopProtocol
}

pub(super) fn runtime_overview_for(
    app_version: String,
    app_data_dir: &Path,
    transport: RuntimeTransport,
) -> RuntimeOverview {
    RuntimeOverview {
        app_version,
        build_mode: if cfg!(debug_assertions) {
            RuntimeBuildMode::Debug
        } else {
            RuntimeBuildMode::Release
        },
        platform: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        transport,
        app_data_path: app_data_dir.to_string_lossy().into_owned(),
    }
}
