//! The application's own provider transfer file format.
//!
//! One exported SQL file carries the exact persisted provider documents of
//! this application: [`ProviderFile`] for Claude providers and the Codex
//! official record, [`CodexProviderFile`] for third-party Codex providers.
//! The row body is the stored document itself, so an import restores every
//! field — model routes, capabilities, native usage scripts, display
//! metadata, and sort positions — with no mapping step and no loss. SQLite
//! is only the carrier: the caller applies the file to a scratch database
//! and hands each row to [`parse_transfer_row`], which validates it back
//! into the typed documents before anything reaches the provider stores.

use crate::contracts::{CodexProviderFile, ProviderFile, RouteMode};

/// Which store a transferred row belongs to. The same spellings are the SQL
/// `kind` column values, so this enum is the single owner of the vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TransferKind {
    #[serde(rename = "claude")]
    Claude,
    #[serde(rename = "codex_official")]
    CodexOfficial,
    #[serde(rename = "codex_custom")]
    CodexCustom,
}

impl TransferKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::CodexOfficial => "codex_official",
            Self::CodexCustom => "codex_custom",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "claude" => Some(Self::Claude),
            "codex_official" => Some(Self::CodexOfficial),
            "codex_custom" => Some(Self::CodexCustom),
            _ => None,
        }
    }
}

/// One validated transfer row's payload: the stored document plus the store
/// it lands in.
#[derive(Debug)]
pub enum TransferProvider {
    Claude(ProviderFile),
    CodexOfficial(ProviderFile),
    CodexCustom(CodexProviderFile),
}

/// One applied and validated database row, ready for preview or import.
#[derive(Debug)]
pub struct TransferRow {
    pub id: String,
    pub kind: TransferKind,
    pub provider: TransferProvider,
}

/// The table every exported file creates. The spelling is owned by this
/// module's writer and reader pair.
const TRANSFER_TABLE: &str = "agent_switchboard_providers";

const TRANSFER_SCHEMA: &str = "CREATE TABLE IF NOT EXISTS agent_switchboard_providers (
  id TEXT PRIMARY KEY,
  kind TEXT NOT NULL,
  position INTEGER NOT NULL,
  file TEXT NOT NULL
);
";

/// Validates one applied database row back into a typed provider document.
/// The `id` and `position` columns must repeat exactly what the embedded
/// document carries, so a hand-edited row fails loudly instead of importing
/// half-agreed facts.
pub fn parse_transfer_row(
    id: &str,
    kind: &str,
    position: i64,
    file: &str,
) -> Result<TransferRow, String> {
    let kind = TransferKind::parse(kind).ok_or_else(|| format!("未知档案类别：{kind}"))?;
    let provider = match kind {
        TransferKind::Claude => TransferProvider::Claude(parse_provider_file(file)?),
        TransferKind::CodexOfficial => {
            let file = parse_provider_file(file)?;
            if file.route_mode != RouteMode::Official {
                return Err("官方登录行必须携带 official 路由档案".to_string());
            }
            TransferProvider::CodexOfficial(file)
        }
        TransferKind::CodexCustom => {
            let file: CodexProviderFile = serde_json::from_str(file)
                .map_err(|error| format!("Codex 供应商档案无法解析：{error}"))?;
            file.validate()?;
            TransferProvider::CodexCustom(file)
        }
    };
    let (document_id, document_position) = match &provider {
        TransferProvider::Claude(file) | TransferProvider::CodexOfficial(file) => {
            (file.id.clone(), file.position)
        }
        TransferProvider::CodexCustom(file) => (file.profile.id.clone(), file.position),
    };
    if document_id != id {
        return Err("行 id 与档案内容不一致".to_string());
    }
    if position < 0 || document_position != position as u64 {
        return Err("行排序位置与档案内容不一致".to_string());
    }
    Ok(TransferRow {
        id: document_id,
        kind,
        provider,
    })
}

fn parse_provider_file(file: &str) -> Result<ProviderFile, String> {
    serde_json::from_str(file).map_err(|error| format!("供应商档案无法解析：{error}"))
}

/// Serializes every stored provider document into one self-contained SQL
/// file. The statement vocabulary is exactly what the applier's authorizer
/// allows: one schema statement and `INSERT OR REPLACE` rows inside one
/// transaction, so re-applying the same file is idempotent.
pub fn write_transfer_sql(
    claude: &[ProviderFile],
    codex_official: &[ProviderFile],
    codex_custom: &[CodexProviderFile],
) -> Result<String, String> {
    fn body(file: &impl serde::Serialize) -> Result<String, String> {
        serde_json::to_string(file).map_err(|_| "供应商档案序列化失败".to_string())
    }

    let mut rows: Vec<(TransferKind, String, u64, String)> = Vec::new();
    for file in claude {
        rows.push((TransferKind::Claude, file.id.clone(), file.position, body(file)?));
    }
    for file in codex_official {
        rows.push((
            TransferKind::CodexOfficial,
            file.id.clone(),
            file.position,
            body(file)?,
        ));
    }
    for file in codex_custom {
        rows.push((
            TransferKind::CodexCustom,
            file.profile.id.clone(),
            file.position,
            body(file)?,
        ));
    }

    let mut sql = String::new();
    sql.push_str("-- Agent Switchboard 供应商导出文件（完整配置）。\n");
    sql.push_str("-- 在其他设备打开「供应商 → 导入 / 导出 → 导入 SQL」选择本文件即可导入。\n");
    sql.push_str("-- 文件包含 API 密钥，请妥善保管。\n");
    sql.push_str("BEGIN TRANSACTION;\n");
    sql.push_str(TRANSFER_SCHEMA);
    for (kind, id, position, file) in rows {
        sql.push_str(&format!(
            "INSERT OR REPLACE INTO {TRANSFER_TABLE} (id, kind, position, file)\nVALUES ({}, {}, {}, {});\n",
            sql_string(&id),
            sql_string(kind.as_str()),
            position,
            sql_string(&file),
        ));
    }
    sql.push_str("COMMIT;\n");
    Ok(sql)
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0f0e0d0c-1a2b-4c3d-8e9f-0a1b2c3d4e5f";

    fn claude_file_json(name: &str) -> String {
        serde_json::json!({
            "id": ID,
            "name": name,
            "position": 100,
            "routeMode": "custom",
            "apiKey": "sk-test",
            "maxOutputTokens": null,
            "parameters": { "settings": {} },
        })
        .to_string()
    }

    #[test]
    fn parse_round_trips_a_claude_row() {
        let row = parse_transfer_row(ID, "claude", 100, &claude_file_json("测试"))
            .expect("row parses");
        assert_eq!(row.id, ID);
        assert_eq!(row.kind, TransferKind::Claude);
        match row.provider {
            TransferProvider::Claude(file) => assert_eq!(file.name, "测试"),
            other => panic!("unexpected provider: {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_disagreeing_rows() {
        assert!(parse_transfer_row(ID, "claude", 200, &claude_file_json("测试")).is_err());
        assert!(parse_transfer_row(ID, "codex_official", 100, &claude_file_json("测试")).is_err());
        assert!(parse_transfer_row(ID, "unknown", 100, &claude_file_json("测试")).is_err());
        assert!(parse_transfer_row(ID, "claude", -1, &claude_file_json("测试")).is_err());
    }

    #[test]
    fn writer_quotes_embedded_documents() {
        let file: ProviderFile = serde_json::from_str(&claude_file_json("名字'引号")).unwrap();
        let sql = write_transfer_sql(&[file], &[], &[]).expect("sql writes");
        assert!(sql.contains(TRANSFER_TABLE));
        // The name sits inside the JSON document, so the SQL-level escaping
        // shows up as the doubled apostrophe inside the file literal.
        assert!(sql.contains("名字''引号"));
        assert!(sql.ends_with("COMMIT;\n"));
    }
}
