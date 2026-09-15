use super::error::{blocking, observe, operation_error, state, CommandError};
use super::switching;
use crate::local_state::LocalState;
use crate::runtime_log::RuntimeLogAction;
use asb_core::contracts::AppKind;
use asb_core::discovery::{self, CodexImportSource, DiscoveryPaths, DiscoveryReport};
use tauri::Manager;

/// Standard user-level configuration locations. This resolver does not read,
/// create, or write any target.
pub fn local_config_paths() -> Result<DiscoveryPaths, String> {
    Ok(DiscoveryPaths {
        codex: LocalState::user_config_path(AppKind::Codex)?
            .to_string_lossy()
            .to_string(),
        codex_auth: LocalState::codex_auth_path()?.to_string_lossy().to_string(),
        claude: LocalState::user_config_path(AppKind::Claude)?
            .to_string_lossy()
            .to_string(),
    })
}

fn read_discovery_file(path: &str) -> Result<Option<String>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) if std::path::Path::new(path).is_file() => Ok(Some(text)),
        Ok(_) => Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("无法读取配置文件".to_string()),
    }
}

/// Re-reads the current Codex files for the import boundary. The returned
/// source may contain a draft and is never serialized to the renderer.
pub(super) fn codex_import_source() -> Result<CodexImportSource, CommandError> {
    let paths = local_config_paths()
        .map_err(|error| CommandError::new("config-path-unavailable", error))?;
    let config = read_discovery_file(&paths.codex)
        .map_err(|error| CommandError::new("codex-import-unavailable", error))?
        .ok_or_else(|| CommandError::new("import-unavailable", "当前没有可读取的 Codex 配置"))?;
    let auth = read_discovery_file(&paths.codex_auth)
        .map_err(|error| CommandError::new("codex-import-unavailable", error))?;
    discovery::codex_import_proposal(&paths.codex, &config, auth.as_deref(), |path| {
        read_discovery_file(&path.to_string_lossy())
    })
    .map_err(|error| CommandError::new("import-unavailable", error))?
    .ok_or_else(|| CommandError::new("import-unavailable", "当前配置没有可导入的 Codex 供应商"))
}

pub(super) fn discovery_report() -> Result<DiscoveryReport, CommandError> {
    let paths = local_config_paths()
        .map_err(|error| CommandError::new("config-path-unavailable", error))?;
    Ok(discovery::discover(&paths, read_discovery_file))
}

/// Read-only discovery of local Codex and Claude Code configuration. Reads at
/// most three files; never writes, creates, or locks any target. A successful
/// scan replaces the local display cache; a cache-write failure is logged and
/// never hides the fresh result.
#[tauri::command]
pub async fn discover_local(app: tauri::AppHandle) -> Result<DiscoveryReport, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let report = discovery_report()?;
        if let Err(error) = state.save_discovery_cache(&report) {
            log::warn!("无法保存发现扫描缓存: {error}");
        }
        Ok(report)
    })
    .await
}

/// The previous successful scan, shown before the next one runs. Null before
/// the first scan ever completed.
#[tauri::command]
pub async fn discover_cached(
    app: tauri::AppHandle,
) -> Result<Option<DiscoveryReport>, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        state
            .load_discovery_cache()
            .map_err(|error| CommandError::new("discovery-cache-unavailable", error))
    })
    .await
}

/// Read-only scan of the local Codex and Claude Code JSONL session stores.
/// The session manager never receives a source path from the renderer, so it
/// cannot be redirected to unrelated user files.
#[tauri::command]
pub async fn list_sessions() -> Result<crate::session_manager::SessionScan, CommandError> {
    blocking(move || Ok(crate::session_manager::scan_sessions())).await
}

/// Loads one transcript after resolving its source from the approved session
/// roots. No session file is created, modified, or deleted by this command.
#[tauri::command]
pub async fn get_session_messages(
    app: AppKind,
    session_id: String,
) -> Result<Vec<crate::session_manager::SessionMessage>, CommandError> {
    blocking(move || {
        crate::session_manager::load_messages(app, &session_id)
            .map_err(|message| CommandError::new("session-unavailable", message))
    })
    .await
}

/// Starts the selected session in the platform terminal. The session source
/// is resolved by the backend; the renderer never submits a path or
/// executable command.
#[tauri::command]
pub async fn resume_session(
    app: AppKind,
    session_id: String,
) -> Result<crate::session_manager::SessionResume, CommandError> {
    observe(RuntimeLogAction::SessionResumed, async move {
        blocking(move || {
            crate::session_manager::resume_session(app, &session_id)
                .map_err(|message| CommandError::new("session-resume-failed", message))
        })
        .await
    })
    .await
}

/// Permanently removes the one local session record the backend resolves from
/// the approved roots. Irreversible by contract; the confirmation sheet in
/// the renderer is the only gate.
#[tauri::command]
pub async fn delete_session(app: AppKind, session_id: String) -> Result<(), CommandError> {
    observe(RuntimeLogAction::SessionDeleted, async move {
        blocking(move || {
            crate::session_manager::delete_session(app, &session_id)
                .map_err(|message| CommandError::new("session-delete-failed", message))
        })
        .await
    })
    .await
}

/// Batch counterpart of `delete_session`: one scan resolves every requested
/// record and each item reports its own outcome, so an unresolved or
/// undeletable id never blocks the rest. Only an unreadable root set fails
/// the whole call.
#[tauri::command]
pub async fn delete_sessions(
    requests: Vec<crate::session_manager::SessionDeleteRequest>,
) -> Result<Vec<crate::session_manager::SessionDeleteOutcome>, CommandError> {
    observe(RuntimeLogAction::SessionDeleted, async move {
        blocking(move || {
            crate::session_manager::delete_sessions(&requests)
                .map_err(|message| CommandError::new("session-delete-failed", message))
        })
        .await
    })
    .await
}

/// Read-only scan of the local external database. Secrets never cross this
/// boundary: the returned items carry routing facts only.
#[tauri::command]
pub async fn scan_ccswitch(
    app: tauri::AppHandle,
) -> Result<crate::ccswitch_source::CcSwitchScan, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        crate::ccswitch_source::scan(&state)
            .map_err(|error| CommandError::new("ccswitch-unavailable", error))
    })
    .await
}

/// Imports selected external Claude providers into the app's own profile
/// store. The import itself never projects to Codex or Claude Code; it first
/// recovers an already-confirmed interrupted profile transaction when one
/// exists. Codex rows are completed in the editor and rejected here.
#[tauri::command]
pub async fn import_ccswitch_claude_profiles(
    app: tauri::AppHandle,
    keys: Vec<String>,
) -> Result<crate::ccswitch_source::CcSwitchImportOutcome, CommandError> {
    observe(RuntimeLogAction::CcSwitchProfilesImported, async move {
        let state = state(&app)?;
        let gateway = app
            .state::<crate::gateway::GatewayController>()
            .inner()
            .clone();
        blocking(move || {
            switching::ensure_profile_save_recovered(&app)?;
            if gateway.has_active_routes() {
                return Err(CommandError::new(
                    "gateway-route-active",
                    "本机协议网关正在使用供应商；请先切换到直连或官方登录后再导入供应商",
                ));
            }
            crate::ccswitch_source::import(&state, &keys)
                .map_err(|error| operation_error("ccswitch-import-failed", error))
        })
        .await
    })
    .await
}
