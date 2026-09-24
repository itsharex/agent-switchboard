use super::error::{blocking, observe, operation_error, state, store_error, CommandError};
use super::switching;
use crate::local_state::LocalState;
use crate::runtime_log::RuntimeLogAction;
use asb_core::contracts::{AppKind, RouteMode};
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
        .ok_or_else(|| {
            CommandError::keyed(
                "import-unavailable",
                "errors.misc.noReadableCodexConfig",
                "当前没有可读取的 Codex 配置",
            )
        })?;
    let auth = read_discovery_file(&paths.codex_auth)
        .map_err(|error| CommandError::new("codex-import-unavailable", error))?;
    discovery::codex_import_proposal(&paths.codex, &config, auth.as_deref(), |path| {
        read_discovery_file(&path.to_string_lossy())
    })
    .map_err(|error| CommandError::new("import-unavailable", error))?
    .ok_or_else(|| {
        CommandError::keyed(
            "import-unavailable",
            "errors.misc.noImportableCodexProviders",
            "当前配置没有可导入的 Codex 供应商",
        )
    })
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

/// Read-only scan of the local external database. `db_directory` is a
/// user-picked folder that directly contains `cc-switch.db`; absence keeps
/// the default home location. Secrets never cross this boundary: the
/// returned items carry routing facts only.
#[tauri::command]
pub async fn scan_ccswitch(
    app: tauri::AppHandle,
    db_directory: Option<String>,
) -> Result<crate::ccswitch_source::CcSwitchScan, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        crate::ccswitch_source::scan(&state, db_directory.as_deref())
            .map_err(|error| CommandError::new("ccswitch-unavailable", error))
    })
    .await
}

/// Imports selected external Claude providers into the app's own profile
/// store. `db_directory` selects the same folder the scan used, so the
/// freshness re-scan reads the database the user previewed. The import itself
/// never projects to Codex or Claude Code; it first recovers an
/// already-confirmed interrupted profile transaction when one exists. Codex
/// rows are completed in the editor and rejected here.
#[tauri::command]
pub async fn import_ccswitch_claude_profiles(
    app: tauri::AppHandle,
    keys: Vec<String>,
    db_directory: Option<String>,
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
                return Err(CommandError::keyed(
                    "gateway-route-active",
                    "errors.misc.gatewayRouteActiveBlockImport",
                    "本机协议网关正在使用供应商；请先切换到直连或官方登录后再导入供应商",
                ));
            }
            crate::ccswitch_source::import(&state, &keys, db_directory.as_deref())
                .map_err(|error| operation_error("ccswitch-import-failed", error))
        })
        .await
    })
    .await
}

/// The report of one SQL export: how many provider documents left this
/// machine and which stored records have no exportable form.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSqlExport {
    pub exported_count: usize,
    pub skipped: Vec<crate::provider_transfer::ProviderSqlSkip>,
}

/// Writes every stored provider document — complete configuration included —
/// to a user-chosen SQL file. The other device applies the file through the
/// companion 「导入 SQL」 panel; no command line is involved. The file carries
/// provider credentials, so the path is the user's own responsibility;
/// nothing is written anywhere else.
#[tauri::command]
pub async fn export_providers_sql(
    app: tauri::AppHandle,
    target_path: String,
) -> Result<ProviderSqlExport, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        let configuration = state.configuration();
        let claude = crate::config_store::providers::load_provider_files(
            &configuration,
            AppKind::Claude,
        )
        .map_err(store_error)?;
        let official_records = crate::config_store::providers::load_provider_files(
            &configuration,
            AppKind::Codex,
        )
        .map_err(store_error)?;
        let mut codex_official = Vec::new();
        let mut skipped = Vec::new();
        for file in official_records {
            if file.route_mode == RouteMode::Official {
                codex_official.push(file);
            } else {
                skipped.push(crate::provider_transfer::ProviderSqlSkip {
                    name: file.name.clone(),
                    reason: "通用存储中的 Codex 自定义档案属于已停用形态，无法导出".to_string(),
                });
            }
        }
        let codex_custom = configuration
            .codex_provider_snapshots()
            .map_err(store_error)?
            .into_iter()
            .map(|(file, _)| file)
            .collect::<Vec<_>>();
        let exported_count = claude.len() + codex_official.len() + codex_custom.len();
        if exported_count == 0 {
            let detail = if skipped.is_empty() {
                CommandError::keyed(
                    "provider-export-empty",
                    "errors.misc.noExportableProviders",
                    "没有可导出的供应商档案",
                )
            } else {
                CommandError::localized(
                    "provider-export-empty",
                    "errors.misc.noExportableProvidersWithSkipped",
                    format!("没有可导出的供应商档案；{} 项无法导出", skipped.len()),
                    serde_json::json!({ "count": skipped.len() }),
                )
            };
            return Err(detail);
        }
        let sql = asb_core::provider_transfer::write_transfer_sql(
            &claude,
            &codex_official,
            &codex_custom,
        )
        .map_err(|error| CommandError::new("provider-export-failed", error))?;
        let path = std::path::PathBuf::from(&target_path);
        if path.is_dir() {
            return Err(CommandError::keyed(
                "provider-export-invalid",
                "errors.misc.exportTargetIsDirectory",
                "导出目标是一个目录，请提供文件路径",
            ));
        }
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.is_dir() {
                return Err(CommandError::keyed(
                    "provider-export-invalid",
                    "errors.misc.exportParentDirMissing",
                    "导出目标的父目录不存在",
                ));
            }
        }
        std::fs::write(&path, sql).map_err(|error| {
            CommandError::localized(
                "provider-export-failed",
                "errors.misc.exportWriteFailed",
                format!("无法写入导出文件：{error}"),
                serde_json::json!({ "error": error.to_string() }),
            )
        })?;
        Ok(ProviderSqlExport {
            exported_count,
            skipped,
        })
    })
    .await
}

/// Applies one exported provider SQL file to the app-owned scratch database
/// and returns the typed preview. Secrets never cross this boundary: the
/// returned items carry routing facts only.
#[tauri::command]
pub async fn apply_providers_sql(
    app: tauri::AppHandle,
    sql_path: String,
) -> Result<crate::provider_transfer::ProviderSqlScan, CommandError> {
    let state = state(&app)?;
    blocking(move || {
        crate::provider_transfer::scan(&state, &sql_path)
            .map_err(|error| CommandError::new("provider-sql-apply-failed", error))
    })
    .await
}

/// Imports selected providers from the same export file the preview applied.
/// Re-applying the file is the freshness guard, so a stale preview can never
/// import. Like every profile-writing command, it first recovers an
/// already-confirmed interrupted profile transaction and refuses while the
/// local protocol gateway is routing.
#[tauri::command]
pub async fn import_providers_sql(
    app: tauri::AppHandle,
    ids: Vec<String>,
    sql_path: String,
) -> Result<crate::provider_transfer::ProviderSqlImportOutcome, CommandError> {
    observe(RuntimeLogAction::ProvidersSqlImported, async move {
        let state = state(&app)?;
        let gateway = app
            .state::<crate::gateway::GatewayController>()
            .inner()
            .clone();
        blocking(move || {
            switching::ensure_profile_save_recovered(&app)?;
            if gateway.has_active_routes() {
                return Err(CommandError::keyed(
                    "gateway-route-active",
                    "errors.misc.gatewayRouteActiveBlockImport",
                    "本机协议网关正在使用供应商；请先切换到直连或官方登录后再导入供应商",
                ));
            }
            crate::provider_transfer::import(&state, &ids, &sql_path)
                .map_err(|error| operation_error("provider-sql-import-failed", error))
        })
        .await
    })
    .await
}
