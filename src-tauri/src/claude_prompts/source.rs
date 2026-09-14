//! Read-only CC Switch Claude prompt import. Source activation never authorizes a live write here.

use super::{contracts::*, list, store, transaction};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::{collections::BTreeSet, path::Path};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeSourcePrompt {
    pub source_id: String,
    pub draft: ClaudePromptDraft,
    pub enabled_in_source: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudePromptSource {
    pub source_revision: String,
    pub prompts: Vec<ClaudeSourcePrompt>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudePromptImportResult {
    pub view: ClaudePromptsView,
    pub imported: usize,
    pub unchanged: usize,
    pub warnings: Vec<String>,
}

pub(crate) fn scan(source: &Path) -> Result<ClaudePromptSource, String> {
    let connection = Connection::open_with_flags(source, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("无法只读打开 Claude 提示词来源：{error}"))?;
    connection
        .busy_timeout(std::time::Duration::from_secs(3))
        .map_err(|e| e.to_string())?;
    let connection = connection
        .unchecked_transaction()
        .map_err(|error| error.to_string())?;
    validate_schema(&connection)?;
    let oversized: i64 = connection.query_row("SELECT COUNT(*) FROM prompts WHERE app_type='claude' AND length(CAST(content AS BLOB)) > 2097152", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if oversized != 0 {
        return Err("来源 Claude 提示词超过单项 2 MiB 限制".into());
    }
    let mut statement = connection.prepare("SELECT id,name,content,description,enabled FROM prompts WHERE app_type='claude' ORDER BY created_at,id LIMIT 1001")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut prompts = Vec::new();
    let mut total_bytes = 0;
    for row in rows {
        let (source_id, name, content, description, enabled) = row.map_err(|e| e.to_string())?;
        if !matches!(enabled, 0 | 1) {
            return Err("来源 Claude 提示词 enabled 必须为 0 或 1".into());
        }
        total_bytes += content.len() + name.len() + description.as_ref().map_or(0, String::len);
        if total_bytes > 8 * 1024 * 1024 {
            return Err("Claude 提示词来源超过 8 MiB 限制".into());
        }
        let draft = ClaudePromptDraft {
            name,
            content,
            description,
        };
        draft.validate()?;
        prompts.push(ClaudeSourcePrompt {
            source_id,
            draft,
            enabled_in_source: enabled == 1,
        });
    }
    if prompts.len() > 1000 {
        return Err("来源 Claude 提示词超过 1000 项限制".into());
    }
    let serialized = serde_json::to_string(&prompts).map_err(|_| "Claude 提示词来源无法编码")?;
    if serialized.len() > 8 * 1024 * 1024 {
        return Err("Claude 提示词来源超过 8 MiB 限制".into());
    }
    Ok(ClaudePromptSource {
        source_revision: asb_switch::sha256_hex(&serialized),
        prompts,
    })
}

pub(crate) fn import(
    root: &Path,
    target: &Path,
    source: &Path,
    ids: &[String],
    source_revision: &str,
    library_revision: &str,
) -> Result<ClaudePromptImportResult, String> {
    transaction::require_clear(root)?;
    if ids.is_empty() || ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err("请选择不重复的 Claude 提示词来源项".into());
    }
    let scanned = scan(source)?;
    if scanned.source_revision != source_revision {
        return Err("CC Switch Claude 提示词来源已改变，请重新扫描".into());
    }
    let (mut library, _) = store::load(root)?;
    let mut imported = 0;
    let mut unchanged = 0;
    for id in ids {
        let item = scanned
            .prompts
            .iter()
            .find(|prompt| &prompt.source_id == id)
            .ok_or("选中的 Claude 提示词来源项不存在")?;
        if let Some(existing) = library
            .prompts
            .iter()
            .find(|p| p.draft.name.to_lowercase() == item.draft.name.to_lowercase())
        {
            if existing.draft != item.draft {
                return Err(format!(
                    "Claude 提示词“{}”与本地同名但内容不同，请先重命名；未导入任何项",
                    item.draft.name
                ));
            }
            unchanged += 1;
        } else {
            library.prompts.push(ClaudePrompt {
                id: uuid::Uuid::new_v4().to_string(),
                draft: item.draft.clone(),
            });
            imported += 1;
        }
    }
    store::save(root, &library, library_revision)?;
    Ok(ClaudePromptImportResult {
        view: list(root, target)?,
        imported,
        unchanged,
        warnings: vec![
            "只导入 Claude 提示词库；来源中的启用状态不写入 CLAUDE.md，需另行预览并确认应用".into(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_import_is_read_only_revision_guarded_idempotent_and_claude_only() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("cc-switch.db");
        let database = Connection::open(&source).unwrap();
        database.execute_batch("CREATE TABLE prompts(id TEXT,app_type TEXT,name TEXT,content TEXT,description TEXT,enabled INTEGER,created_at INTEGER); INSERT INTO prompts VALUES ('c','claude','Claude preset','hello',NULL,1,1),('x','codex','Not Claude','never import',NULL,1,2);").unwrap();
        let snapshot = scan(&source).unwrap();
        assert_eq!(snapshot.prompts.len(), 1);
        let target = dir.path().join("CLAUDE.md");
        let before = std::fs::read(&source).unwrap();
        let empty = list(dir.path(), &target).unwrap();
        let result = import(
            dir.path(),
            &target,
            &source,
            &["c".into()],
            &snapshot.source_revision,
            &empty.file_hash,
        )
        .unwrap();
        assert_eq!(result.imported, 1);
        assert_eq!(result.view.active_prompt_id, None);
        assert!(!target.exists());
        let again = import(
            dir.path(),
            &target,
            &source,
            &["c".into()],
            &snapshot.source_revision,
            &result.view.file_hash,
        )
        .unwrap();
        assert_eq!(again.imported, 0);
        assert_eq!(again.unchanged, 1);
        assert_eq!(std::fs::read(&source).unwrap(), before);
        database
            .execute("UPDATE prompts SET content='changed' WHERE id='c'", [])
            .unwrap();
        assert!(import(
            dir.path(),
            &target,
            &source,
            &["c".into()],
            &snapshot.source_revision,
            &again.view.file_hash
        )
        .is_err());
    }
}

fn validate_schema(connection: &Connection) -> Result<(), String> {
    let fields = connection
        .prepare("PRAGMA table_info(prompts)")
        .map_err(|e| e.to_string())?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| e.to_string())?
        .collect::<Result<BTreeSet<_>, _>>()
        .map_err(|e| e.to_string())?;
    for field in [
        "id",
        "app_type",
        "name",
        "content",
        "description",
        "enabled",
        "created_at",
    ] {
        if !fields.contains(field) {
            return Err(format!("来源不是支持的 CC Switch 提示词表：缺少 {field}"));
        }
    }
    Ok(())
}
