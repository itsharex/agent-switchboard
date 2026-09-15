//! Claude-only read of the source endpoint-candidate table. Upstream
//! regenerates each provider's `meta.custom_endpoints` from this table on
//! every read, so the table is the authoritative candidate set; when the
//! table exists it replaces meta-derived candidates exactly like the
//! source's own read path. Codex rows keep their existing meta-derived
//! candidates and are never fed by this module.

use super::db::table_exists;
use asb_core::contracts::{ProviderDraft, ProviderEndpoint};
use rusqlite::Connection;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub(super) struct SourceEndpoint {
    pub(super) url: String,
    pub(super) added_at: i64,
}

pub(super) fn read(
    connection: &Connection,
) -> Result<BTreeMap<String, Vec<SourceEndpoint>>, String> {
    if !table_exists(connection, "provider_endpoints")? {
        return Ok(BTreeMap::new());
    }
    let mut statement = connection
        .prepare(
            "SELECT provider_id, url, added_at FROM provider_endpoints \
             WHERE app_type = 'claude' ORDER BY added_at ASC, url ASC",
        )
        .map_err(|error| format!("无法读取 Claude 测速候选: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                row.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(|error| format!("无法读取 Claude 测速候选: {error}"))?;
    let mut map = BTreeMap::new();
    for row in rows {
        let (provider_id, url, added_at) =
            row.map_err(|error| format!("Claude 测速候选格式无效: {error}"))?;
        let (provider_id, url) = (provider_id.trim().to_string(), url.trim().to_string());
        if provider_id.is_empty() || url.is_empty() {
            continue;
        }
        map.entry(provider_id)
            .or_insert_with(Vec::new)
            .push(SourceEndpoint {
                url,
                added_at: added_at.unwrap_or(0),
            });
    }
    Ok(map)
}

/// Replaces meta-derived candidates with the table's set for one Claude
/// draft, mirroring the source's own read path.
pub(super) fn merge_into(draft: &mut ProviderDraft, endpoints: &[SourceEndpoint]) {
    draft.connection.custom_endpoints = endpoints
        .iter()
        .map(|endpoint| {
            let value = ProviderEndpoint {
                url: endpoint.url.clone(),
                added_at: endpoint.added_at,
                last_used: None,
            };
            (endpoint.url.clone(), value)
        })
        .collect();
}

#[cfg(test)]
mod tests;
