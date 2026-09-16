//! Read-only the source application shared-snippet import into the local client settings.
//! Credentials and provider-owned routing never enter the shared stores; the
//! write only replaces application state, never a real client file.

use asb_core::claude_common::{self, Extra, SharedSnippet};
use asb_core::contracts::{AppKind, ClientSettingsSnapshot};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use std::{collections::BTreeMap, path::Path};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSnippetPreview {
    pub visual: Vec<ClaudeSnippetVisualKey>,
    pub extra_keys: Vec<String>,
    pub rejected: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSnippetVisualKey {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSnippetSource {
    pub found: bool,
    pub source_revision: String,
    pub preview: Option<ClaudeSnippetPreview>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSnippetImport {
    pub snapshot: ClientSettingsSnapshot,
    pub visual_changed: usize,
    pub visual_unchanged: usize,
    pub extra_changed: usize,
    pub rejected: Vec<String>,
    pub warnings: Vec<String>,
}

const SNIPPET_KEY: &str = "common_config_claude";

pub(crate) fn scan(source: &Path) -> Result<ClaudeSnippetSource, String> {
    let (snippet, parsed) = read_parsed(source)?;
    let Some(snippet) = snippet else {
        return Ok(ClaudeSnippetSource {
            found: false,
            source_revision: asb_switch::sha256_hex(""),
            preview: None,
        });
    };
    Ok(ClaudeSnippetSource {
        found: true,
        source_revision: asb_switch::sha256_hex(&snippet),
        preview: Some(preview_of(parsed)),
    })
}

pub(crate) fn import(
    store: &crate::config_store::ConfigStore,
    source: &Path,
    source_revision: &str,
    expected_settings_hash: &str,
) -> Result<ClaudeSnippetImport, String> {
    let (snippet, parsed) = read_parsed(source)?;
    let Some(snippet) = snippet else {
        return Err("来源没有 Claude 通用配置片段".into());
    };
    if asb_switch::sha256_hex(&snippet) != source_revision {
        return Err("通用配置导入源已改变，请重新扫描".into());
    }
    let current = store
        .get_client_settings(AppKind::Claude)
        .map_err(|error| error.to_string())?;
    if current.settings_hash != expected_settings_hash {
        return Err("客户端设置已更新，请重新读取后再导入".into());
    }
    let mut settings = current.settings.clone();
    let (visual_changed, visual_unchanged) =
        claude_common::apply_visual(&mut settings, &parsed.visual);
    let (extra, extra_changed) = claude_common::merged_extra(&settings.claude_extra, &parsed.extra);
    settings.claude_extra = extra;
    let snapshot = store
        .save_client_settings(AppKind::Claude, settings, expected_settings_hash)
        .map_err(|error| error.to_string())?;
    let mut rejected = parsed.rejected.clone();
    rejected.sort();
    let mut warnings = vec![
        "只写入应用内的客户端偏好与通用片段；真实 Claude 配置仍由切换预览确认后应用".to_string(),
    ];
    warnings.extend(rejected.iter().map(|name| format!("未导入: {name}")));
    Ok(ClaudeSnippetImport {
        snapshot,
        visual_changed,
        visual_unchanged,
        extra_changed,
        rejected,
        warnings,
    })
}

fn read_parsed(source: &Path) -> Result<(Option<String>, SharedSnippet), String> {
    let connection = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("无法只读打开导入源数据库：{error}"))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(3))
        .map_err(|error| error.to_string())?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    validate_schema(&transaction)?;
    let snippet: Option<String> = transaction
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [SNIPPET_KEY],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| format!("无法读取通用配置片段：{error}"))?
        .flatten();
    let snippet = snippet.filter(|text| !text.trim().is_empty());
    match &snippet {
        Some(text) => Ok((Some(text.clone()), claude_common::import_shared(text)?)),
        None => Ok((None, empty_parse())),
    }
}

fn empty_parse() -> SharedSnippet {
    SharedSnippet {
        visual: BTreeMap::new(),
        extra: Extra::new(),
        rejected: Vec::new(),
    }
}

fn preview_of(parsed: SharedSnippet) -> ClaudeSnippetPreview {
    let mut extra_keys = claude_common::extra_leaf_paths(&parsed.extra);
    extra_keys.sort();
    let mut visual = parsed
        .visual
        .iter()
        .map(|(key, value)| ClaudeSnippetVisualKey {
            key: key.clone(),
            value: value.display(),
        })
        .collect::<Vec<_>>();
    visual.sort_by(|a, b| a.key.cmp(&b.key));
    let mut rejected = parsed.rejected;
    rejected.sort();
    ClaudeSnippetPreview {
        visual,
        extra_keys,
        rejected,
    }
}

#[cfg(test)]
mod tests;

fn validate_schema(connection: &Connection) -> Result<(), String> {
    let fields = connection
        .prepare("PRAGMA table_info(settings)")
        .map_err(|e| e.to_string())?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<std::collections::BTreeSet<_>, _>>()
        .map_err(|e| e.to_string())?;
    for field in ["key", "value"] {
        if !fields.contains(field) {
            return Err(format!("来源不是支持的设置表：缺少 {field}"));
        }
    }
    Ok(())
}
