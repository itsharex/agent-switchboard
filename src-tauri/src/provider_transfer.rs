//! Applies exported provider SQL files and imports the restored records.
//!
//! An exported file is applied only to the app-owned scratch database under
//! `state/provider-import/` — never to the user's home databases — inside an
//! authorizer that denies every statement the exporter never emits. Rows are
//! then validated back into the typed provider documents
//! ([`asb_core::provider_transfer`]) and previewed without secrets. Importing
//! re-applies the same file first — the freshness guard — then writes each
//! selected record through the stores' validating file boundary, so another
//! device receives the complete stored configuration.

use crate::config_store::{providers::load_provider_files, StoreOperationError, PROVIDER_POSITION_STEP};
use crate::local_state::LocalState;
use asb_core::contracts::{AppKind, RouteMode};
use asb_core::provider_transfer::{parse_transfer_row, TransferKind, TransferProvider, TransferRow};
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// The largest exported file accepted for applying.
const MAX_SQL_BYTES: u64 = 32 * 1024 * 1024;
const SCRATCH_DIRECTORY: &str = "provider-import";
const SCRATCH_FILE: &str = "agent-switchboard.db";
const TRANSFER_TABLE: &str = "agent_switchboard_providers";

/// One named item the transfer could not carry or import.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSqlSkip {
    pub name: String,
    pub reason: String,
}

/// One importable provider summary with the target store's duplicate marking
/// applied. This is the complete scan response contract: only fields shown in
/// the renderer, never an API key.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSqlScanItem {
    pub key: String,
    pub kind: TransferKind,
    pub name: String,
    pub model: Option<String>,
    pub base_url: Option<String>,
    pub warnings: Vec<String>,
    /// A record with the same id already exists; importing overwrites it in
    /// place at its stored position.
    pub existing: bool,
}

/// Full preview of one applied export file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSqlScan {
    pub sql_path: String,
    pub providers: Vec<ProviderSqlScanItem>,
    pub skipped: Vec<ProviderSqlSkip>,
}

/// Outcome of a batch import.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSqlImportOutcome {
    pub imported_count: usize,
    pub updated_count: usize,
    pub not_imported: Vec<ProviderSqlSkip>,
}

/// Applies the export file at `sql_path` to the scratch database and previews
/// its rows against the current stores.
pub fn scan(state: &LocalState, sql_path: &str) -> Result<ProviderSqlScan, String> {
    let sql_path = sql_path.trim();
    let (rows, skipped) = apply_and_parse(state, sql_path)?;
    let configuration = state.configuration();
    let claude = load_provider_files(&configuration, AppKind::Claude)
        .map_err(|error| error.to_string())?;
    let official = load_provider_files(&configuration, AppKind::Codex)
        .map_err(|error| error.to_string())?;
    let custom = configuration
        .codex_provider_snapshots()
        .map_err(|error| error.to_string())?;
    let providers = rows
        .into_iter()
        .map(|row| {
            let TransferRow { id, kind, provider } = row;
            let (name, model, base_url, existing, warnings) = match provider {
                TransferProvider::Claude(file) => {
                    let mut warnings = Vec::new();
                    if file.route_mode == RouteMode::Official
                        && !claude.iter().any(|stored| stored.id == id)
                        && claude
                            .iter()
                            .any(|stored| stored.route_mode == RouteMode::Official)
                    {
                        warnings
                            .push("本机已有其他 Claude 官方登录记录，导入时将跳过".to_string());
                    }
                    (
                        file.name.clone(),
                        file.model.clone(),
                        file.base_url.clone(),
                        claude.iter().any(|stored| stored.id == id),
                        warnings,
                    )
                }
                TransferProvider::CodexOfficial(file) => {
                    let mut warnings = Vec::new();
                    if !official.iter().any(|stored| stored.id == id)
                        && official
                            .iter()
                            .any(|stored| stored.route_mode == RouteMode::Official)
                    {
                        warnings.push("本机已有其他官方登录记录，导入时将跳过".to_string());
                    }
                    (
                        file.name.clone(),
                        file.model.clone(),
                        file.base_url.clone(),
                        official.iter().any(|stored| stored.id == id),
                        warnings,
                    )
                }
                TransferProvider::CodexCustom(file) => (
                    file.profile.name.clone(),
                    Some(file.profile.default_model.clone()),
                    Some(file.profile.endpoint.0.clone()),
                    custom.iter().any(|(stored, _)| stored.profile.id == id),
                    Vec::new(),
                ),
            };
            ProviderSqlScanItem {
                key: id,
                kind,
                name,
                model,
                base_url,
                warnings,
                existing,
            }
        })
        .collect();
    Ok(ProviderSqlScan {
        sql_path: sql_path.to_string(),
        providers,
        skipped,
    })
}

/// Re-applies the same file the preview used (so a stale preview can never
/// import) and writes every requested record through the stores' validating
/// file boundary. Existing ids are overwritten in place at their stored
/// position; new records append after the target group's highest position,
/// preserving the exported order.
pub fn import(
    state: &LocalState,
    ids: &[String],
    sql_path: &str,
) -> Result<ProviderSqlImportOutcome, StoreOperationError> {
    let (rows, skipped) = apply_and_parse(state, sql_path.trim())?;
    let requested: HashSet<&str> = ids.iter().map(String::as_str).collect();
    let configuration = state.configuration();
    let claude = load_provider_files(&configuration, AppKind::Claude)?;
    let official = load_provider_files(&configuration, AppKind::Codex)?;
    let custom = configuration.codex_provider_snapshots()?;
    let mut claude_next = claude.iter().map(|file| file.position).max().unwrap_or(0);
    let mut codex_next = official
        .iter()
        .map(|file| file.position)
        .chain(custom.iter().map(|(file, _)| file.position))
        .max()
        .unwrap_or(0);
    let mut outcome = ProviderSqlImportOutcome {
        imported_count: 0,
        updated_count: 0,
        not_imported: Vec::new(),
    };
    for row in &rows {
        if !requested.contains(row.id.as_str()) {
            continue;
        }
        match &row.provider {
            TransferProvider::Claude(source) => {
                let existing = claude.iter().any(|stored| stored.id == row.id);
                // The official record is a per-client singleton in the store
                // and in every validated snapshot; another device's record
                // never replaces this machine's own.
                if !existing
                    && source.route_mode == RouteMode::Official
                    && claude
                        .iter()
                        .any(|stored| stored.route_mode == RouteMode::Official)
                {
                    outcome.not_imported.push(ProviderSqlSkip {
                        name: source.name.clone(),
                        reason: "本机已有其他 Claude 官方登录记录".to_string(),
                    });
                    continue;
                }
                let mut file = source.clone();
                match claude.iter().find(|stored| stored.id == row.id) {
                    Some(existing) => file.position = existing.position,
                    None => {
                        claude_next += PROVIDER_POSITION_STEP;
                        file.position = claude_next;
                    }
                }
                configuration.overwrite_provider_file(AppKind::Claude, file)?;
                if existing {
                    outcome.updated_count += 1;
                } else {
                    outcome.imported_count += 1;
                }
            }
            TransferProvider::CodexOfficial(source) => {
                let mut file = source.clone();
                if let Some(existing) = official.iter().find(|stored| stored.id == row.id) {
                    file.position = existing.position;
                    configuration.overwrite_provider_file(AppKind::Codex, file)?;
                    outcome.updated_count += 1;
                } else if official
                    .iter()
                    .any(|stored| stored.route_mode == RouteMode::Official)
                {
                    // The official record is a per-client singleton; another
                    // device's record never replaces this machine's own.
                    outcome.not_imported.push(ProviderSqlSkip {
                        name: file.name.clone(),
                        reason: "本机已有其他官方登录记录".to_string(),
                    });
                } else {
                    codex_next += PROVIDER_POSITION_STEP;
                    file.position = codex_next;
                    configuration.overwrite_provider_file(AppKind::Codex, file)?;
                    outcome.imported_count += 1;
                }
            }
            TransferProvider::CodexCustom(source) => {
                let existing = custom
                    .iter()
                    .any(|(stored, _)| stored.profile.id == row.id);
                let mut file = source.clone();
                match custom.iter().find(|(stored, _)| stored.profile.id == row.id) {
                    Some((existing, _)) => file.position = existing.position,
                    None => {
                        codex_next += PROVIDER_POSITION_STEP;
                        file.position = codex_next;
                    }
                }
                configuration.overwrite_codex_provider_file(file)?;
                if existing {
                    outcome.updated_count += 1;
                } else {
                    outcome.imported_count += 1;
                }
            }
        }
    }
    for id in ids {
        if rows.iter().any(|row| row.id == *id) {
            continue;
        }
        let reason = skipped
            .iter()
            .find(|skip| skip.name == *id)
            .map(|skip| skip.reason.clone())
            .unwrap_or_else(|| "预览结果已变化，请重新选择文件".to_string());
        outcome.not_imported.push(ProviderSqlSkip {
            name: id.clone(),
            reason,
        });
    }
    Ok(outcome)
}

fn scratch_path(state: &LocalState) -> PathBuf {
    state.root().join(SCRATCH_DIRECTORY).join(SCRATCH_FILE)
}

/// Removes the scratch database and its journal sidecars so one apply never
/// observes leftovers of a previous file.
fn reset_scratch(scratch: &Path) -> Result<(), String> {
    let parent = scratch.parent().ok_or("导入暂存数据库路径无效")?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("无法创建导入暂存目录：{error}"))?;
    let file = scratch
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("导入暂存数据库路径无效")?;
    for suffix in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(parent.join(format!("{file}{suffix}")));
    }
    Ok(())
}

/// Allows exactly the statement vocabulary the exporter emits. CREATE TABLE
/// additionally writes schema rows (`sqlite_master` updates) and builds an
/// implicit autoindex for its PRIMARY KEY constraint, so those two actions
/// are part of the exporter's own vocabulary — the autoindex guard is
/// airtight because SQLite reserves the `sqlite_` name prefix. Everything
/// else — ATTACH, PRAGMA, function calls, hand-written UPDATE or CREATE
/// INDEX — is denied.
fn deny_escaping_statements(context: AuthContext<'_>) -> Authorization {
    match context.action {
        AuthAction::CreateTable { .. }
        | AuthAction::Insert { .. }
        | AuthAction::Delete { .. }
        | AuthAction::Read { .. }
        | AuthAction::Select
        | AuthAction::Transaction { .. }
        | AuthAction::Update { table_name: "sqlite_master", .. } => Authorization::Allow,
        AuthAction::CreateIndex { index_name, .. } if index_name.starts_with("sqlite_autoindex_") => {
            Authorization::Allow
        }
        _ => Authorization::Deny,
    }
}

/// Applies the file to the reset scratch database and validates every row
/// back into typed provider documents. Rows that fail validation become named
/// skips instead of aborting the whole preview.
fn apply_and_parse(
    state: &LocalState,
    sql_path: &str,
) -> Result<(Vec<TransferRow>, Vec<ProviderSqlSkip>), String> {
    let path = Path::new(sql_path);
    if !path.is_file() {
        return Err("所选文件不存在".to_string());
    }
    let size = std::fs::metadata(path)
        .map_err(|error| format!("无法读取所选文件：{error}"))?
        .len();
    if size > MAX_SQL_BYTES {
        return Err("导出的 SQL 文件超过 32MB 上限".to_string());
    }
    let sql = std::fs::read_to_string(path)
        .map_err(|error| format!("无法读取所选文件：{error}"))?;
    let scratch = scratch_path(state);
    reset_scratch(&scratch)?;
    let connection = Connection::open_with_flags(
        &scratch,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )
    .map_err(|error| format!("无法打开导入暂存数据库：{error}"))?;
    connection.authorizer(Some(deny_escaping_statements));
    connection.execute_batch(&sql).map_err(|error| {
        if error.to_string().contains("not authorized") {
            "所选文件包含 Agent Switchboard 导出格式之外的语句，已拒绝应用".to_string()
        } else {
            format!("无法应用所选 SQL 文件：{error}")
        }
    })?;
    let not_export_file = || "所选文件不是 Agent Switchboard 导出的供应商文件".to_string();
    let mut statement = connection
        .prepare(&format!(
            "SELECT id, kind, position, file FROM {TRANSFER_TABLE} ORDER BY position, id"
        ))
        .map_err(|_| not_export_file())?;
    let applied = statement
        .query_map([], |record| {
            Ok((
                record.get::<_, String>(0)?,
                record.get::<_, String>(1)?,
                record.get::<_, i64>(2)?,
                record.get::<_, String>(3)?,
            ))
        })
        .map_err(|_| not_export_file())?;
    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    for entry in applied {
        let (id, kind, position, file) =
            entry.map_err(|error| format!("无法读取导入暂存数据：{error}"))?;
        if Uuid::parse_str(&id).is_err() {
            skipped.push(ProviderSqlSkip {
                name: id,
                reason: "档案 id 不是有效的 UUID".to_string(),
            });
            continue;
        }
        match parse_transfer_row(&id, &kind, position, &file) {
            Ok(row) => rows.push(row),
            Err(reason) => skipped.push(ProviderSqlSkip { name: id, reason }),
        }
    }
    Ok((rows, skipped))
}

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::contracts::{
        CodexCatalogEntry, CodexEndpoint, CodexModelRoute, CodexProviderDraft, CodexProviderFile,
        CodexUpstream, ProviderConnectionOptions, ProviderDraft, ProviderEndpoint, ProviderFile,
        ResponsesRequestMode, UsageQuery, DEFAULT_CODEX_CAPABILITIES,
    };
    use asb_core::ownership::default_provider_parameters;

    const TRANSFER_TABLE_DDL: &str = "CREATE TABLE IF NOT EXISTS agent_switchboard_providers (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  position INTEGER NOT NULL,
  file TEXT NOT NULL
);";

    /// Every store and scratch database below lives inside the sandbox root;
    /// nothing in this module touches the real app state or home directory.
    fn sandbox(tag: &str) -> (tempfile::TempDir, LocalState) {
        let dir = tempfile::Builder::new()
            .prefix(&format!("asb-transfer-{tag}-"))
            .tempdir()
            .expect("sandbox dir");
        let state = LocalState::from_root(dir.path().to_path_buf());
        state.initialize_schemas().expect("schemas");
        (dir, state)
    }

    fn claude_relay_draft(name: &str, api_key: &str) -> ProviderDraft {
        let mut draft: ProviderDraft = serde_json::from_value(serde_json::json!({
            "app": "claude",
            "routeMode": "custom",
            "name": name,
            "baseUrl": "https://relay.example.com",
            "apiKey": api_key,
            "upstreamProtocol": "anthropicMessages",
            "maxOutputTokens": null,
            "model": "claude-sonnet-4-5",
            "parameters": { "settings": {} },
            "notes": "迁移测试备注",
            "websiteUrl": "https://example.com",
            "display": { "icon": "relay", "iconColor": "#3366cc", "category": "中转", "createdAt": 1758000000 },
            "usageQuery": {
                "kind": "declarative",
                "url": "{{baseUrl}}/usage",
                "remainingPath": "data/balance",
                "unit": "USD",
                "refreshIntervalMinutes": 60
            }
        }))
        .expect("claude draft parses");
        draft.parameters = default_provider_parameters(AppKind::Claude);
        draft.connection.custom_endpoints.insert(
            "https://backup.example.com".to_string(),
            ProviderEndpoint {
                url: "https://backup.example.com".to_string(),
                added_at: 1758000000,
                last_used: None,
            },
        );
        draft
    }

    /// Same shape the cc-switch importer uses for its official Claude row.
    fn claude_official_draft() -> ProviderDraft {
        let mut draft: ProviderDraft = serde_json::from_value(serde_json::json!({
            "app": "claude",
            "routeMode": "official",
            "name": "Claude 官方登录",
            "apiKey": "",
            "maxOutputTokens": null,
            "parameters": { "settings": {} }
        }))
        .expect("claude official draft parses");
        draft.parameters = default_provider_parameters(AppKind::Claude);
        draft
    }

    fn codex_custom_draft() -> CodexProviderDraft {
        CodexProviderDraft {
            name: "第三方 Codex".to_string(),
            endpoint: CodexEndpoint("https://api.example.com/v1".to_string()),
            api_key: "sk-codex-1".to_string(),
            authentication: None,
            connection: ProviderConnectionOptions::default(),
            upstream: CodexUpstream::ChatCompletions,
            request_mode: ResponsesRequestMode::Standard,
            default_model: "gpt-5.2".to_string(),
            catalog: vec![
                CodexCatalogEntry::default_entry("gpt-5.2"),
                CodexCatalogEntry::default_entry("gpt-5.2-mini"),
            ],
            model_routes: vec![CodexModelRoute {
                client_model: "gpt-5.2-mini".to_string(),
                upstream_model: "provider-model-x".to_string(),
            }],
            subagent_route: None,
            capabilities: DEFAULT_CODEX_CAPABILITIES,
            parameters: default_provider_parameters(AppKind::Codex),
            notes: None,
            website_url: None,
            usage_query: Some(UsageQuery::Declarative {
                url: "{{baseUrl}}/v1/usage".to_string(),
                remaining_path: Some("data/remaining".to_string()),
                used_path: None,
                total_path: None,
                unit: Some("USD".to_string()),
                refresh_interval_minutes: 30,
            }),
        }
    }

    fn seed_source_device(state: &LocalState) {
        let configuration = state.configuration();
        configuration
            .create_provider(claude_relay_draft("中转 A", "sk-a"))
            .expect("claude A seeds");
        configuration
            .create_provider(claude_relay_draft("中转 B", "sk-b"))
            .expect("claude B seeds");
        let (official, created) = configuration
            .ensure_codex_official_record()
            .expect("codex official seeds");
        assert!(created, "the sandbox source device starts without official");
        let _ = official.profile.id;
        configuration
            .create_codex_provider(codex_custom_draft())
            .expect("codex custom seeds");
    }

    fn export_sql(state: &LocalState, path: &Path) {
        let configuration = state.configuration();
        let claude = load_provider_files(&configuration, AppKind::Claude).expect("claude files");
        let official = load_provider_files(&configuration, AppKind::Codex).expect("official files");
        let custom = configuration
            .codex_provider_snapshots()
            .expect("custom files")
            .into_iter()
            .map(|(file, _)| file)
            .collect::<Vec<_>>();
        let sql =
            asb_core::provider_transfer::write_transfer_sql(&claude, &official, &custom)
                .expect("sql writes");
        std::fs::write(path, sql).expect("sql file writes");
    }

    fn strip_positions_claude(files: &[ProviderFile]) -> Vec<ProviderFile> {
        let mut stripped = files.to_vec();
        for file in &mut stripped {
            file.position = 0;
        }
        stripped
    }

    fn strip_positions_custom(entries: &[(CodexProviderFile, String)]) -> Vec<CodexProviderFile> {
        entries
            .iter()
            .map(|(file, _)| {
                let mut file = file.clone();
                file.position = 0;
                file
            })
            .collect()
    }

    fn import_everything(state: &LocalState, sql_path: &Path) -> ProviderSqlImportOutcome {
        let scan = scan(state, sql_path.to_str().unwrap()).expect("scan");
        let ids: Vec<String> = scan.providers.iter().map(|item| item.key.clone()).collect();
        import(state, &ids, sql_path.to_str().unwrap()).expect("import")
    }

    #[test]
    fn transfer_round_trips_complete_configuration_into_a_fresh_device() {
        let (dir_a, state_a) = sandbox("round-a");
        seed_source_device(&state_a);
        let sql_path = dir_a.path().join("agent-switchboard-providers.sql");
        export_sql(&state_a, &sql_path);

        let expected_claude =
            load_provider_files(&state_a.configuration(), AppKind::Claude).unwrap();
        let expected_official =
            load_provider_files(&state_a.configuration(), AppKind::Codex).unwrap();
        let expected_custom = state_a.configuration().codex_provider_snapshots().unwrap();

        let (_dir_b, state_b) = sandbox("round-b");
        let scan = scan(&state_b, sql_path.to_str().unwrap()).expect("scan");
        assert_eq!(scan.providers.len(), 4);
        assert!(scan.providers.iter().all(|item| !item.existing));
        assert!(scan.providers.iter().all(|item| item.warnings.is_empty()));
        assert!(scan.skipped.is_empty());
        assert_eq!(
            scan.providers
                .iter()
                .filter(|item| item.kind == TransferKind::Claude)
                .count(),
            2
        );
        assert_eq!(
            scan.providers
                .iter()
                .filter(|item| item.kind == TransferKind::CodexOfficial)
                .count(),
            1
        );
        let custom_item = scan
            .providers
            .iter()
            .find(|item| item.kind == TransferKind::CodexCustom)
            .expect("custom item");
        assert_eq!(custom_item.name, "第三方 Codex");
        assert_eq!(custom_item.base_url.as_deref(), Some("https://api.example.com/v1"));
        assert_eq!(custom_item.model.as_deref(), Some("gpt-5.2"));

        let outcome = import_everything(&state_b, &sql_path);
        assert_eq!(outcome.imported_count, 4);
        assert_eq!(outcome.updated_count, 0);
        assert!(outcome.not_imported.is_empty());

        let store = state_b.configuration();
        let claude = load_provider_files(&store, AppKind::Claude).unwrap();
        let official = load_provider_files(&store, AppKind::Codex).unwrap();
        let custom = store.codex_provider_snapshots().unwrap();
        // Complete configuration: every stored field round-trips; only the
        // sort positions may compact on the fresh device.
        assert_eq!(
            strip_positions_claude(&claude),
            strip_positions_claude(&expected_claude)
        );
        assert_eq!(
            strip_positions_claude(&official),
            strip_positions_claude(&expected_official)
        );
        assert_eq!(
            strip_positions_custom(&custom),
            strip_positions_custom(&expected_custom)
        );
        assert_eq!(
            claude.iter().map(|file| file.name.clone()).collect::<Vec<_>>(),
            vec!["中转 A".to_string(), "中转 B".to_string()]
        );
        assert_eq!(official.len(), 1);
        assert_eq!(official[0].id, expected_official[0].id);
        let mut positions: Vec<u64> = official
            .iter()
            .map(|file| file.position)
            .chain(custom.iter().map(|(file, _)| file.position))
            .collect();
        positions.sort_unstable();
        assert_eq!(positions, vec![100, 200]);
        // The scratch database never leaves the sandbox state root.
        assert!(state_b.root().join(SCRATCH_DIRECTORY).is_dir());
    }

    #[test]
    fn re_importing_over_existing_records_updates_in_place_without_position_drift() {
        let (dir_a, state_a) = sandbox("update-a");
        seed_source_device(&state_a);
        let sql_path = dir_a.path().join("agent-switchboard-providers.sql");
        export_sql(&state_a, &sql_path);

        let (_dir_b, state_b) = sandbox("update-b");
        let first = import_everything(&state_b, &sql_path);
        assert_eq!(first.imported_count, 4);

        let store = state_b.configuration();
        let before: Vec<(String, u64)> = load_provider_files(&store, AppKind::Claude)
            .unwrap()
            .into_iter()
            .map(|file| (file.id.clone(), file.position))
            .chain(
                load_provider_files(&store, AppKind::Codex)
                    .unwrap()
                    .into_iter()
                    .map(|file| (file.id.clone(), file.position)),
            )
            .chain(
                store
                    .codex_provider_snapshots()
                    .unwrap()
                    .into_iter()
                    .map(|(file, _)| (file.profile.id.clone(), file.position)),
            )
            .collect();

        let scan = scan(&state_b, sql_path.to_str().unwrap()).expect("re-scan");
        assert!(scan.providers.iter().all(|item| item.existing));
        let outcome = import_everything(&state_b, &sql_path);
        assert_eq!(outcome.imported_count, 0);
        assert_eq!(outcome.updated_count, 4);
        assert!(outcome.not_imported.is_empty());

        let store = state_b.configuration();
        let after: Vec<(String, u64)> = load_provider_files(&store, AppKind::Claude)
            .unwrap()
            .into_iter()
            .map(|file| (file.id.clone(), file.position))
            .chain(
                load_provider_files(&store, AppKind::Codex)
                    .unwrap()
                    .into_iter()
                    .map(|file| (file.id.clone(), file.position)),
            )
            .chain(
                store
                    .codex_provider_snapshots()
                    .unwrap()
                    .into_iter()
                    .map(|(file, _)| (file.profile.id.clone(), file.position)),
            )
            .collect();
        assert_eq!(before, after, "overwrites keep the local sort positions");
    }

    #[test]
    fn importing_keeps_the_target_devices_own_official_record() {
        let (dir_a, state_a) = sandbox("official-a");
        seed_source_device(&state_a);
        let sql_path = dir_a.path().join("agent-switchboard-providers.sql");
        export_sql(&state_a, &sql_path);

        let (_dir_c, state_c) = sandbox("official-c");
        let (own_official, created) = state_c
            .configuration()
            .ensure_codex_official_record()
            .expect("target official seeds");
        assert!(created);

        let scan = scan(&state_c, sql_path.to_str().unwrap()).expect("scan");
        let official_item = scan
            .providers
            .iter()
            .find(|item| item.kind == TransferKind::CodexOfficial)
            .expect("official item");
        assert!(!official_item.existing);
        assert!(official_item
            .warnings
            .iter()
            .any(|warning| warning.contains("官方登录记录")));

        let outcome = import_everything(&state_c, &sql_path);
        assert_eq!(outcome.imported_count, 3);
        assert_eq!(outcome.not_imported.len(), 1);
        assert!(outcome.not_imported[0].reason.contains("官方登录记录"));

        let official = load_provider_files(&state_c.configuration(), AppKind::Codex).unwrap();
        assert_eq!(official.len(), 1);
        assert_eq!(official[0].id, own_official.profile.id);
    }

    #[test]
    fn apply_rejects_escaping_foreign_and_tampered_files() {
        let (dir, state) = sandbox("hostile");

        let attach = dir.path().join("attach.sql");
        std::fs::write(&attach, "BEGIN; ATTACH DATABASE 'evil.db' AS x; COMMIT;")
            .expect("attach fixture");
        let error = scan(&state, attach.to_str().unwrap()).expect_err("attach is rejected");
        assert!(error.contains("导出格式之外的语句"), "got: {error}");

        let foreign = dir.path().join("foreign.sql");
        std::fs::write(&foreign, "CREATE TABLE other (x TEXT);").expect("foreign fixture");
        let error = scan(&state, foreign.to_str().unwrap()).expect_err("foreign is rejected");
        assert!(error.contains("不是 Agent Switchboard 导出"), "got: {error}");

        let tampered = dir.path().join("tampered.sql");
        std::fs::write(
            &tampered,
            format!(
                "BEGIN TRANSACTION;\n{TRANSFER_TABLE_DDL}\
                 INSERT OR REPLACE INTO agent_switchboard_providers (id, kind, position, file)\n\
                 VALUES ('0f0e0d0c-1a2b-4c3d-8e9f-0a1b2c3d4e5f', 'claude', 100, \
                 '{{\"id\":\"11111111-2222-4333-8444-555555555555\",\"name\":\"x\",\"position\":100,\
                 \"routeMode\":\"custom\",\"apiKey\":\"\",\"maxOutputTokens\":null,\
                 \"parameters\":{{\"settings\":{{}}}}}}');\n\
                 COMMIT;\n"
            ),
        )
        .expect("tampered fixture");
        let tampered_scan = scan(&state, tampered.to_str().unwrap()).expect("tampered rows are skipped");
        assert!(tampered_scan.providers.is_empty());
        assert_eq!(tampered_scan.skipped.len(), 1);
        assert!(tampered_scan.skipped[0].reason.contains("不一致"), "got: {}", tampered_scan.skipped[0].reason);

        let escaping = dir.path().join("escaping.sql");
        for (name, statement) in [
            ("pragma", "PRAGMA journal_mode = WAL;"),
            (
                "user update",
                &format!("{TRANSFER_TABLE_DDL}UPDATE agent_switchboard_providers SET position = 1;"),
            ),
            (
                "hand-written index",
                &format!("{TRANSFER_TABLE_DDL}CREATE INDEX probe ON agent_switchboard_providers (kind);"),
            ),
        ] {
            std::fs::write(&escaping, statement).expect("escaping fixture");
            let error = scan(&state, escaping.to_str().unwrap())
                .expect_err(&format!("{name} is rejected"));
            assert!(
                error.contains("导出格式之外的语句"),
                "{name} was not denied: {error}"
            );
        }
    }

    #[test]
    fn importing_keeps_the_target_devices_own_claude_official_record() {
        let (dir_a, state_a) = sandbox("claude-official-a");
        seed_source_device(&state_a);
        state_a
            .configuration()
            .create_provider(claude_official_draft())
            .expect("claude official seeds");
        let sql_path = dir_a.path().join("agent-switchboard-providers.sql");
        export_sql(&state_a, &sql_path);

        let (_dir_c, state_c) = sandbox("claude-official-c");
        let own = state_c
            .configuration()
            .create_provider(claude_official_draft())
            .expect("target claude official seeds")
            .profile
            .id
            .clone();

        let scan = scan(&state_c, sql_path.to_str().unwrap()).expect("scan");
        let official_item = scan
            .providers
            .iter()
            .find(|item| {
                item.kind == TransferKind::Claude
                    && item
                        .warnings
                        .iter()
                        .any(|warning| warning.contains("官方登录记录"))
            })
            .expect("claude official item carries the singleton warning");
        assert!(!official_item.existing);

        let outcome = import_everything(&state_c, &sql_path);
        assert_eq!(outcome.imported_count, 4);
        assert_eq!(outcome.not_imported.len(), 1);
        assert!(outcome.not_imported[0].reason.contains("Claude 官方登录记录"));

        let claude = load_provider_files(&state_c.configuration(), AppKind::Claude).unwrap();
        let officials: Vec<&ProviderFile> = claude
            .iter()
            .filter(|file| file.route_mode == RouteMode::Official)
            .collect();
        assert_eq!(officials.len(), 1);
        assert_eq!(officials[0].id, own);
    }
}
