//! Source Claude order is independent of Codex's import and local profile order.
use asb_core::ccswitch::CcSwitchRow;
use rusqlite::Connection;
use std::collections::BTreeMap;
pub(super) fn reorder(connection: &Connection, rows: &mut [CcSwitchRow]) -> Result<(), String> {
    let mut columns = connection
        .prepare("PRAGMA table_info(providers)")
        .map_err(|e| format!("无法读取来源表结构: {e}"))?;
    let names = columns
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("无法读取来源表结构: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("无法读取来源表结构: {e}"))?;
    if !names.iter().any(|key| key == "sort_index") {
        return Ok(());
    }
    let mut query = connection
        .prepare("SELECT id, sort_index FROM providers WHERE app_type='claude'")
        .map_err(|e| format!("无法读取 Claude 来源顺序: {e}"))?;
    let positions = query
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
        })
        .map_err(|e| format!("无法读取 Claude 来源顺序: {e}"))?
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|e| format!("Claude 来源顺序格式无效: {e}"))?;
    let mut claude = rows
        .iter()
        .filter(|row| row.app_type == "claude")
        .cloned()
        .collect::<Vec<_>>();
    claude.sort_by_key(|row| {
        let index = positions.get(&row.id).copied().flatten();
        (index.is_none(), index)
    });
    let mut ordered = claude.into_iter();
    for row in rows.iter_mut().filter(|row| row.app_type == "claude") {
        *row = ordered.next().expect("same Claude row count");
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_order_and_wal_commits_are_visible_without_changing_the_source() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("source#fixture.db");
        let writer = Connection::open(&path).unwrap();
        writer.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0; CREATE TABLE providers(id TEXT,app_type TEXT,name TEXT,settings_config TEXT,website_url TEXT,notes TEXT,meta TEXT,sort_index INTEGER);").unwrap();
        for (id, app, index) in [
            ("late", "claude", Some(2)),
            ("codex", "codex", None),
            ("first", "claude", Some(1)),
        ] {
            let settings = if app == "claude" {
                r#"{"env":{"ANTHROPIC_BASE_URL":"https://fixture.invalid","ANTHROPIC_AUTH_TOKEN":"fake-key"}}"#
            } else {
                r#"{"config":"model = \"gpt-5\""}"#
            };
            writer.execute("INSERT INTO providers(id,app_type,name,settings_config,sort_index) VALUES(?1,?2,?1,?3,?4)",rusqlite::params![id,app,settings,index]).unwrap();
        }
        let scan = super::super::db::scan_db(&path).unwrap();
        let names = scan
            .proposals
            .iter()
            .filter(|p| p.draft.app() == asb_core::AppKind::Claude)
            .map(|p| p.draft.name())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["first", "late"]);
        let reader = super::super::db::open_read_only(&path).unwrap();
        assert!(reader.execute("DELETE FROM providers", []).is_err());
        writer
            .execute(
                "UPDATE providers SET name='committed-in-wal' WHERE id='first'",
                [],
            )
            .unwrap();
        assert!(super::super::db::scan_db(&path)
            .unwrap()
            .proposals
            .iter()
            .any(|p| p.draft.name() == "committed-in-wal"));
        let count: i64 = writer
            .query_row("SELECT count(*) FROM providers", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }
}
