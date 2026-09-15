//! Read-only CC Switch MCP import into the extension library (Claude 专属入口)。
//! 来源 `mcp_servers` 表的每行是一份 Claude 形态的服务定义外加各客户端启用位；
//! 这里只读取并转换成扩展库定义，不部署、不写任何客户端文件，来源启用位只作提示。

use asb_core::extensions::contracts::{
    ExtensionDefinition, ExtensionPayload, McpDefinition, McpMetadata, EXTENSIONS_SCHEMA_VERSION,
};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::{Map, Value};
use std::collections::BTreeSet;
use std::path::Path;

/// Presentation-only keys the source UI stores inside `server_config`.
const SOURCE_UI_KEYS: [&str; 8] = [
    "enabled",
    "source",
    "id",
    "name",
    "description",
    "tags",
    "homepage",
    "docs",
];
const MAX_ROWS: usize = 500;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSourceMcpServer {
    pub source_id: String,
    pub name: String,
    pub transport: String,
    pub enabled_for_claude: bool,
    pub description: Option<String>,
    /// Why this row cannot be imported, when it cannot.
    pub problem: Option<String>,
    /// A library definition with the same key already exists.
    pub existing: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeMcpSource {
    pub source_revision: String,
    pub servers: Vec<ClaudeSourceMcpServer>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeMcpImportResult {
    pub imported: Vec<String>,
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

struct SourceRow {
    id: String,
    name: String,
    config: String,
    description: Option<String>,
    homepage: Option<String>,
    docs: Option<String>,
    tags: Vec<String>,
    enabled_for_claude: bool,
}

struct Candidate {
    row: SourceRow,
    definition: Result<(McpDefinition, McpMetadata), String>,
}

fn open(source: &Path) -> Result<Connection, String> {
    let connection = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("无法只读打开 CC Switch 数据库：{error}"))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(3))
        .map_err(|error| error.to_string())?;
    Ok(connection)
}

fn read_rows(connection: &Connection) -> Result<Vec<SourceRow>, String> {
    let exists: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='mcp_servers'",
            [],
            |row| row.get(0),
        )
        .ok();
    if exists.is_none() {
        return Err("来源没有 mcp_servers 表（需要 CC Switch 3.7+ 的统一 MCP 存储）".into());
    }
    let mut statement = connection
        .prepare(
            "SELECT id, name, server_config, description, homepage, docs, tags, enabled_claude \
             FROM mcp_servers ORDER BY name, id LIMIT 501",
        )
        .map_err(|error| format!("无法读取 mcp_servers 表：{error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(SourceRow {
                id: row.get::<_, String>(0)?,
                name: row.get::<_, String>(1)?,
                config: row.get::<_, String>(2)?,
                description: row.get::<_, Option<String>>(3)?,
                homepage: row.get::<_, Option<String>>(4)?,
                docs: row.get::<_, Option<String>>(5)?,
                tags: row
                    .get::<_, Option<String>>(6)?
                    .and_then(|text| serde_json::from_str::<Vec<String>>(&text).ok())
                    .unwrap_or_default(),
                enabled_for_claude: row.get::<_, Option<i64>>(7)?.unwrap_or(0) == 1,
            })
        })
        .map_err(|error| format!("无法读取 mcp_servers 表：{error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("无法读取 mcp_servers 表：{error}"))?;
    if rows.len() > MAX_ROWS {
        return Err(format!("来源 MCP 服务超过 {MAX_ROWS} 项限制"));
    }
    Ok(rows)
}

fn convert(row: &SourceRow) -> Result<(McpDefinition, McpMetadata), String> {
    let mut spec: Map<String, Value> = serde_json::from_str::<Value>(&row.config)
        .ok()
        .and_then(|value| value.as_object().cloned())
        .ok_or("server_config 不是 JSON 对象")?;
    // The source sometimes nests the spec under `server`; the UI keys never
    // belong to the launch definition.
    if let Some(Value::Object(inner)) = spec.get("server").cloned() {
        spec = inner;
    }
    for key in SOURCE_UI_KEYS {
        spec.remove(key);
    }
    let key = row.name.trim();
    if key.is_empty() {
        return Err("服务名称为空".into());
    }
    let document = serde_json::json!({ "mcpServers": { key: Value::Object(spec) } }).to_string();
    let definition = asb_core::extensions::mcp::import_claude_server(&document, key)
        .map_err(|error| error.to_string())?;
    let metadata = McpMetadata {
        display_name: None,
        description: row
            .description
            .clone()
            .filter(|text| !text.trim().is_empty()),
        tags: row.tags.clone(),
        homepage: row.homepage.clone().filter(|text| !text.trim().is_empty()),
        docs: row.docs.clone().filter(|text| !text.trim().is_empty()),
    };
    Ok((definition, metadata))
}

fn transport_label(definition: &Result<(McpDefinition, McpMetadata), String>) -> String {
    match definition {
        Ok((McpDefinition::Stdio { .. }, _)) => "stdio",
        Ok((McpDefinition::Http { .. }, _)) => "http",
        Ok((McpDefinition::ClaudeSse { .. }, _)) => "sse",
        Ok((McpDefinition::ClaudeWs { .. }, _)) => "ws",
        Err(_) => "unknown",
    }
    .to_string()
}

fn candidates(source: &Path) -> Result<Vec<Candidate>, String> {
    let connection = open(source)?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    Ok(read_rows(&transaction)?
        .into_iter()
        .map(|row| Candidate {
            definition: convert(&row),
            row,
        })
        .collect())
}

fn revision(candidates: &[Candidate]) -> String {
    let material: Vec<(&str, &str, &str, bool)> = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.row.id.as_str(),
                candidate.row.name.as_str(),
                candidate.row.config.as_str(),
                candidate.row.enabled_for_claude,
            )
        })
        .collect();
    asb_switch::sha256_hex(&serde_json::to_string(&material).unwrap_or_default())
}

pub(crate) fn scan(
    source: &Path,
    library: &[ExtensionDefinition],
) -> Result<ClaudeMcpSource, String> {
    let candidates = candidates(source)?;
    let servers = candidates
        .iter()
        .map(|candidate| ClaudeSourceMcpServer {
            source_id: candidate.row.id.clone(),
            name: candidate.row.name.clone(),
            transport: transport_label(&candidate.definition),
            enabled_for_claude: candidate.row.enabled_for_claude,
            description: candidate.row.description.clone(),
            problem: candidate.definition.as_ref().err().cloned(),
            existing: library
                .iter()
                .any(|definition| definition.name == candidate.row.name),
        })
        .collect();
    Ok(ClaudeMcpSource {
        source_revision: revision(&candidates),
        servers,
    })
}

/// Builds the definitions to create. Rows whose key already exists in the
/// library are reported, never renamed or overwritten; an unchanged payload
/// counts as already present.
pub(crate) fn plan_import(
    source: &Path,
    ids: &[String],
    source_revision: &str,
    library: &[ExtensionDefinition],
    now: &str,
    new_id: impl Fn() -> String,
) -> Result<(Vec<ExtensionDefinition>, ClaudeMcpImportResult), String> {
    if ids.is_empty() || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err("请选择不重复的 CC Switch MCP 服务".into());
    }
    let candidates = candidates(source)?;
    if revision(&candidates) != source_revision {
        return Err("CC Switch MCP 来源已改变，请重新扫描".into());
    }
    let mut definitions = Vec::new();
    let mut result = ClaudeMcpImportResult {
        imported: Vec::new(),
        unchanged: 0,
        warnings: vec!["只导入到扩展库；部署到 Claude 仍需在扩展工作区预览并确认".into()],
    };
    for id in ids {
        let candidate = candidates
            .iter()
            .find(|candidate| &candidate.row.id == id)
            .ok_or("选中的 CC Switch MCP 服务不存在")?;
        let (payload, metadata) = match &candidate.definition {
            Ok(converted) => converted.clone(),
            Err(problem) => {
                result
                    .warnings
                    .push(format!("未导入 {}：{problem}", candidate.row.name));
                continue;
            }
        };
        if let Some(existing) = library
            .iter()
            .find(|definition| definition.name == candidate.row.name)
        {
            if existing.payload == ExtensionPayload::Mcp(payload.clone()) {
                result.unchanged += 1;
            } else {
                result.warnings.push(format!(
                    "未导入 {}：扩展库已有同名但内容不同的定义，请先重命名",
                    candidate.row.name
                ));
            }
            continue;
        }
        let definition = ExtensionDefinition {
            schema_version: EXTENSIONS_SCHEMA_VERSION,
            id: new_id(),
            name: candidate.row.name.clone(),
            mcp_metadata: Some(metadata),
            revision: 1,
            created_at: now.to_string(),
            updated_at: now.to_string(),
            payload: ExtensionPayload::Mcp(payload),
        };
        asb_core::extensions::validate::validate_definition(&definition)
            .map_err(|error| format!("{}：{}", candidate.row.name, error.message))?;
        result.imported.push(definition.name.clone());
        definitions.push(definition);
    }
    Ok((definitions, result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::extensions::contracts::SecretValue;

    fn source(rows: &[(&str, &str, &str, i64)]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cc-switch.db");
        let database = Connection::open(&path).unwrap();
        database
            .execute_batch(
                "CREATE TABLE mcp_servers (id TEXT PRIMARY KEY, name TEXT NOT NULL, server_config TEXT NOT NULL, \
                 description TEXT, homepage TEXT, docs TEXT, tags TEXT NOT NULL DEFAULT '[]', \
                 enabled_claude BOOLEAN NOT NULL DEFAULT 0, enabled_codex BOOLEAN NOT NULL DEFAULT 0);",
            )
            .unwrap();
        for (id, name, config, claude) in rows {
            database
                .execute(
                    "INSERT INTO mcp_servers (id, name, server_config, description, homepage, tags, enabled_claude) \
                     VALUES (?1, ?2, ?3, 'desc', 'https://example.test', '[\"docs\"]', ?4)",
                    rusqlite::params![id, name, config, claude],
                )
                .unwrap();
        }
        (dir, path)
    }

    #[test]
    fn rows_convert_through_the_claude_importer_with_ui_keys_stripped_and_problems_named() {
        let (_dir, path) = source(&[
            (
                "a",
                "docs",
                r#"{"type":"stdio","command":"cmd","args":["/c","npx","-y","docs"],"env":{"TOKEN":"${TOKEN}"},"enabled":true,"source":"preset"}"#,
                1,
            ),
            (
                "b",
                "events",
                r#"{"type":"http","url":"https://mcp.example.test","headers":{"X":"1"}}"#,
                0,
            ),
            ("c", "broken", r#"{"type":"stdio"}"#, 1),
            (
                "d",
                "nested",
                r#"{"server":{"command":"uvx","args":["x"]},"enabled":false}"#,
                1,
            ),
        ]);
        let scanned = scan(&path, &[]).unwrap();
        let names: Vec<_> = scanned
            .servers
            .iter()
            .map(|s| {
                (
                    s.name.as_str(),
                    s.transport.as_str(),
                    s.enabled_for_claude,
                    s.problem.is_some(),
                )
            })
            .collect();
        assert_eq!(
            names,
            vec![
                ("broken", "unknown", true, true),
                ("docs", "stdio", true, false),
                ("events", "http", false, false),
                ("nested", "stdio", true, false),
            ]
        );
        let (definitions, result) = plan_import(
            &path,
            &["a".into(), "b".into(), "c".into(), "d".into()],
            &scanned.source_revision,
            &[],
            "2026-09-15T00:00:00.000Z",
            || "ext-fixture".into(),
        )
        .unwrap();
        assert_eq!(result.imported, vec!["docs", "events", "nested"]);
        assert!(result
            .warnings
            .iter()
            .any(|w| w.starts_with("未导入 broken")));
        match &definitions[0].payload {
            ExtensionPayload::Mcp(McpDefinition::Stdio {
                command, args, env, ..
            }) => {
                // The Windows wrapper is unwrapped to the portable form.
                assert_eq!(command, "npx");
                assert_eq!(args, &["-y", "docs"]);
                assert_eq!(
                    env.get("TOKEN"),
                    Some(&SecretValue::EnvRef {
                        name: "TOKEN".into()
                    })
                );
            }
            other => panic!("unexpected payload {other:?}"),
        }
        let metadata = definitions[0].mcp_metadata.as_ref().unwrap();
        assert_eq!(metadata.description.as_deref(), Some("desc"));
        assert_eq!(metadata.tags, ["docs"]);
    }

    #[test]
    fn revision_and_existing_definitions_guard_the_import() {
        let (_dir, path) = source(&[("a", "docs", r#"{"command":"npx","args":["-y","docs"]}"#, 1)]);
        let scanned = scan(&path, &[]).unwrap();
        assert!(
            plan_import(&path, &["a".into()], "stale", &[], "t", || "x".into())
                .unwrap_err()
                .contains("重新扫描")
        );
        assert!(plan_import(
            &path,
            &["a".into(), "a".into()],
            &scanned.source_revision,
            &[],
            "t",
            || "x".into()
        )
        .is_err());
        let (definitions, _) = plan_import(
            &path,
            &["a".into()],
            &scanned.source_revision,
            &[],
            "t",
            || "ext-1".into(),
        )
        .unwrap();
        let rescanned = scan(&path, &definitions).unwrap();
        assert!(rescanned.servers[0].existing);
        let (again, result) = plan_import(
            &path,
            &["a".into()],
            &scanned.source_revision,
            &definitions,
            "t",
            || "ext-2".into(),
        )
        .unwrap();
        assert!(again.is_empty());
        assert_eq!(result.unchanged, 1);
        let mut changed = definitions.clone();
        changed[0].payload = ExtensionPayload::Mcp(McpDefinition::Http {
            url: "https://x".into(),
            headers: Default::default(),
            bearer: None,
        });
        let (none, result) = plan_import(
            &path,
            &["a".into()],
            &scanned.source_revision,
            &changed,
            "t",
            || "ext-3".into(),
        )
        .unwrap();
        assert!(none.is_empty());
        assert!(result.warnings.iter().any(|w| w.contains("同名但内容不同")));
    }
}
