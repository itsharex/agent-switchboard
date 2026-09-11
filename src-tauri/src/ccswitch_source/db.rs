use asb_core::ccswitch::{self, CcSwitchRow};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};

/// Raw scan outcome before the store marks duplicates.
pub(super) struct RawScan {
    pub(super) proposals: Vec<ccswitch::CcSwitchProposal>,
    pub(super) skipped: Vec<ccswitch::CcSwitchSkip>,
}

/// Locates the source database under the user home directory.
pub(super) fn db_path() -> Result<PathBuf, String> {
    let home = crate::local_state::user_home_dir()?;
    Ok(Path::new(&home).join(".cc-switch").join("cc-switch.db"))
}

pub(super) fn open_read_only(path: &Path) -> Result<Connection, String> {
    if !path.is_file() {
        return Err("未找到 CC Switch 数据库(需要 CC Switch 3.x 已创建数据)".to_string());
    }
    let uri = format!(
        "file:{}?mode=ro&immutable=1",
        path.to_string_lossy().replace('\\', "/")
    );
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    Connection::open_with_flags(&uri, flags).map_err(|error| format!("无法打开数据库: {error}"))
}

/// Reads every provider row in stable `rowid` order. The source schema owns
/// optional query scripts in `meta`.
fn read_rows(connection: &Connection) -> Result<Vec<CcSwitchRow>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, app_type, name, settings_config, website_url, notes, meta \
             FROM providers ORDER BY rowid",
        )
        .map_err(|error| format!("无法读取 providers 表: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(CcSwitchRow {
                id: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                app_type: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                name: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                settings_config: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                website_url: row.get::<_, Option<String>>(4)?,
                notes: row.get::<_, Option<String>>(5)?,
                meta: row.get::<_, Option<String>>(6)?,
            })
        })
        .map_err(|error| format!("无法读取 providers 表: {error}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("无法读取 providers 表: {error}"))
}

pub(super) fn scan_db(path: &Path) -> Result<RawScan, String> {
    let connection = open_read_only(path)?;
    let mut proposals = Vec::new();
    let mut skipped = Vec::new();
    for row in read_rows(&connection)? {
        match ccswitch::map_row(&row) {
            Ok(mut proposal) => {
                if has_invalid_usage_query(&proposal) {
                    clear_usage_query(&mut proposal);
                    proposal
                        .warnings
                        .push("未导入: meta.usage_script.code（无法转换为本应用脚本）".to_string());
                }
                proposals.push(proposal);
            }
            Err(skip) => skipped.push(skip),
        }
    }
    Ok(RawScan { proposals, skipped })
}

fn has_invalid_usage_query(proposal: &ccswitch::CcSwitchProposal) -> bool {
    let usage_query = match &proposal.draft {
        ccswitch::CcSwitchProviderDraft::Claude(draft) => draft.usage_query.as_ref(),
        ccswitch::CcSwitchProviderDraft::Codex(draft) => draft.usage_query.as_ref(),
        // Official rows never carry a usage query.
        ccswitch::CcSwitchProviderDraft::CodexOfficial(_) => None,
    };
    usage_query.is_some_and(|query| crate::usage_query::validate_persisted(query).is_err())
}

fn clear_usage_query(proposal: &mut ccswitch::CcSwitchProposal) {
    match &mut proposal.draft {
        ccswitch::CcSwitchProviderDraft::Claude(draft) => draft.usage_query = None,
        ccswitch::CcSwitchProviderDraft::Codex(draft) => draft.usage_query = None,
        ccswitch::CcSwitchProviderDraft::CodexOfficial(_) => {}
    }
}
