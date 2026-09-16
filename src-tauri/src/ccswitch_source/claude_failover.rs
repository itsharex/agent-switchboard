//! Claude-only the source application failover import: the source queue order and proxy
//! policy become this application's Claude failover policy. Source health
//! rows are never imported — health is computed live here — and the source's
//! current-provider marker is ignored, because the local route is observed
//! from real client files rather than carried over from another tool.

use super::db::{open_read_only, table_exists};
use crate::gateway::failover::{self, ClaudeFailoverPolicy, MAX_RETRIES};
use crate::local_state::LocalState;
use asb_core::ccswitch::{self, CcSwitchProviderDraft, CcSwitchRow};
use asb_core::contracts::{AppKind, RouteMode};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeFailoverQueueMember {
    pub source_name: String,
    pub base_url: Option<String>,
    pub model: Option<String>,
    pub matched_profile_id: Option<String>,
    pub matched_profile_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeFailoverSourceScan {
    pub found: bool,
    pub source_revision: String,
    pub policy_revision: String,
    pub members: Vec<ClaudeFailoverQueueMember>,
    pub proposal: ClaudeFailoverPolicy,
    pub warnings: Vec<String>,
}

struct QueueRow {
    id: String,
    name: String,
    sort_index: Option<i64>,
    settings_config: String,
    meta: Option<String>,
}

struct SourceProxy {
    enabled: bool,
    auto_failover_enabled: bool,
    max_retries: i64,
    streaming_first_byte_timeout: i64,
    streaming_idle_timeout: i64,
    non_streaming_timeout: i64,
    circuit_failure_threshold: i64,
    circuit_success_threshold: i64,
    circuit_timeout_seconds: i64,
    circuit_error_rate_threshold: f64,
    circuit_min_requests: i64,
}

struct Facts {
    queue: Vec<QueueRow>,
    proxy: Option<SourceProxy>,
    revision: String,
}

/// Read-only scan: the ordered source queue with local match states plus the
/// proposed policy. Nothing local is written.
pub(crate) fn scan(source: &Path, state: &LocalState) -> Result<ClaudeFailoverSourceScan, String> {
    let connection = open_read_only(source)?;
    let facts = read_facts(&connection)?;
    propose(facts, state)
}

/// Confirmed import: re-reads the source, re-checks both revisions, and
/// returns the proposed policy for the caller's validate/save/refresh path.
pub(crate) fn import(
    source: &Path,
    source_revision: &str,
    expected_policy_hash: &str,
    state: &LocalState,
) -> Result<(ClaudeFailoverPolicy, Vec<String>), String> {
    let connection = open_read_only(source)?;
    let facts = read_facts(&connection)?;
    if facts.revision != source_revision {
        return Err("故障转移导入源已改变，请重新扫描".into());
    }
    if policy_revision(state)? != expected_policy_hash {
        return Err("Claude 故障转移策略已更新，请重新读取后再导入".into());
    }
    let proposed = propose(facts, state)?;
    Ok((proposed.proposal, proposed.warnings))
}

fn propose(facts: Facts, state: &LocalState) -> Result<ClaudeFailoverSourceScan, String> {
    let current = failover::load(state.root())
        .map_err(|error| format!("本机 Claude 故障转移策略不可读：{error}"))?;
    let mut policy = current;
    let mut warnings = Vec::new();
    let mut members = Vec::new();
    let mut ids = Vec::new();
    for row in &facts.queue {
        let member = member_for(row, state, &mut warnings);
        if let Some(id) = member.matched_profile_id.clone() {
            ids.push(id);
        }
        members.push(member);
    }
    policy.provider_ids = ids;
    if let Some(proxy) = &facts.proxy {
        apply_proxy(&mut policy, proxy, &mut warnings);
    }
    Ok(ClaudeFailoverSourceScan {
        found: !members.is_empty() || facts.proxy.is_some(),
        source_revision: facts.revision,
        policy_revision: policy_revision(state)?,
        proposal: policy,
        members,
        warnings,
    })
}

fn member_for(
    row: &QueueRow,
    state: &LocalState,
    warnings: &mut Vec<String>,
) -> ClaudeFailoverQueueMember {
    let source = CcSwitchRow {
        id: row.id.clone(),
        app_type: "claude".into(),
        name: row.name.clone(),
        settings_config: row.settings_config.clone(),
        website_url: None,
        notes: None,
        display: None,
        meta: row.meta.clone(),
    };
    let empty = ClaudeFailoverQueueMember {
        source_name: row.name.clone(),
        base_url: None,
        model: None,
        matched_profile_id: None,
        matched_profile_name: None,
    };
    let proposal = match ccswitch::map_row(&source) {
        Ok(proposal) => proposal,
        Err(reason) => {
            warnings.push(format!("未导入: {}（{}）", row.name, reason.reason));
            return empty;
        }
    };
    let draft = match proposal.draft {
        CcSwitchProviderDraft::Claude(draft) => draft,
        _ => return empty,
    };
    let matched = state
        .configuration()
        .find_routing_match(&draft)
        .filter(|record| record.profile.app == AppKind::Claude);
    let native = matched
        .as_ref()
        .is_some_and(|record| record.profile.connection.claude_native.is_some());
    let routed = matched.filter(|record| {
        record.profile.route_mode == RouteMode::Custom
            && record.profile.connection.claude_native.is_none()
    });
    if draft.route_mode == RouteMode::Official {
        warnings.push(format!(
            "未加入队列: {}（官方登录不参与故障转移队列）",
            row.name
        ));
    } else if native {
        warnings.push(format!(
            "未加入队列: {}（原生云 SDK 供应商不参与本机队列）",
            row.name
        ));
    } else if routed.is_none() {
        warnings.push(format!(
            "未加入队列: {}（本地没有相同路由的档案，请先导入该供应商）",
            row.name
        ));
    }
    ClaudeFailoverQueueMember {
        source_name: row.name.clone(),
        base_url: draft.base_url.clone(),
        model: draft.model.clone(),
        matched_profile_id: routed.as_ref().map(|record| record.profile.id.clone()),
        matched_profile_name: routed.as_ref().map(|record| record.profile.name.clone()),
    }
}

fn apply_proxy(policy: &mut ClaudeFailoverPolicy, proxy: &SourceProxy, warnings: &mut Vec<String>) {
    policy.enabled = proxy.auto_failover_enabled;
    policy.takeover = proxy.enabled;
    if (0..=MAX_RETRIES as i64).contains(&proxy.max_retries) {
        policy.max_retries = proxy.max_retries as u32;
    } else {
        warnings.push(format!(
            "来源重试次数 {} 超出本机允许范围 0–{MAX_RETRIES}，保留本地值",
            proxy.max_retries
        ));
    }
    let traffic = &mut policy.traffic;
    take(
        &mut traffic.streaming_first_byte_timeout_seconds,
        proxy.streaming_first_byte_timeout,
        3600,
        "流式首包超时",
        warnings,
    );
    take(
        &mut traffic.streaming_idle_timeout_seconds,
        proxy.streaming_idle_timeout,
        3600,
        "流式空闲超时",
        warnings,
    );
    take(
        &mut traffic.non_streaming_timeout_seconds,
        proxy.non_streaming_timeout,
        7200,
        "非流式总超时",
        warnings,
    );
    take(
        &mut traffic.circuit_failure_threshold,
        proxy.circuit_failure_threshold,
        100,
        "熔断失败阈值",
        warnings,
    );
    take(
        &mut traffic.circuit_success_threshold,
        proxy.circuit_success_threshold,
        100,
        "熔断恢复阈值",
        warnings,
    );
    take(
        &mut traffic.circuit_cooldown_seconds,
        proxy.circuit_timeout_seconds,
        3600,
        "熔断冷却时间",
        warnings,
    );
    take(
        &mut traffic.circuit_min_requests,
        proxy.circuit_min_requests,
        10000,
        "错误率最小请求数",
        warnings,
    );
    // The source stores an error-rate fraction (0..1); the local contract
    // stores a percent. A zero fraction can never be expressed locally, so
    // the local value survives with a named warning.
    let percent = (proxy.circuit_error_rate_threshold * 100.0).round() as i64;
    if (1..=100).contains(&percent) {
        traffic.circuit_error_rate_percent = percent as u32;
    } else {
        warnings.push(format!(
            "来源熔断错误率 {} 无法换算为本机百分比档位，保留本地值",
            proxy.circuit_error_rate_threshold
        ));
    }
}

fn take(target: &mut u32, source: i64, max: u32, name: &str, warnings: &mut Vec<String>) {
    if source > 0 && source <= i64::from(max) {
        *target = source as u32;
    } else {
        warnings.push(format!(
            "来源{name} {source} 超出本机允许范围 1–{max}，保留本地值"
        ));
    }
}

fn read_facts(connection: &Connection) -> Result<Facts, String> {
    let queue = read_queue(connection)?;
    let proxy = read_proxy(connection)?;
    let rows = queue
        .iter()
        .map(|row| {
            serde_json::json!({
                "id": row.id,
                "sortIndex": row.sort_index,
                "settings": row.settings_config,
                "meta": row.meta,
            })
        })
        .collect::<Vec<_>>();
    let revision = asb_switch::sha256_hex(
        &serde_json::json!({ "queue": rows, "proxy": proxy.as_ref().map(proxy_json) }).to_string(),
    );
    Ok(Facts {
        queue,
        proxy,
        revision,
    })
}

fn proxy_json(proxy: &SourceProxy) -> serde_json::Value {
    serde_json::json!({
        "enabled": proxy.enabled,
        "autoFailover": proxy.auto_failover_enabled,
        "maxRetries": proxy.max_retries,
        "firstByte": proxy.streaming_first_byte_timeout,
        "idle": proxy.streaming_idle_timeout,
        "nonStreaming": proxy.non_streaming_timeout,
        "failure": proxy.circuit_failure_threshold,
        "success": proxy.circuit_success_threshold,
        "cooldown": proxy.circuit_timeout_seconds,
        "errorRate": proxy.circuit_error_rate_threshold,
        "minRequests": proxy.circuit_min_requests,
    })
}

fn read_queue(connection: &Connection) -> Result<Vec<QueueRow>, String> {
    if !column_exists(connection, "providers", "in_failover_queue")? {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare(
            "SELECT id, name, sort_index, settings_config, meta FROM providers \
             WHERE app_type = 'claude' AND in_failover_queue = 1 \
             ORDER BY COALESCE(sort_index, 999999), id ASC",
        )
        .map_err(|error| format!("无法读取 Claude 故障转移队列: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok(QueueRow {
                id: row.get(0)?,
                name: row.get(1)?,
                sort_index: row.get(2)?,
                settings_config: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
                meta: row.get(4)?,
            })
        })
        .map_err(|error| format!("无法读取 Claude 故障转移队列: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Claude 故障转移队列格式无效: {error}"))?;
    Ok(rows)
}

fn read_proxy(connection: &Connection) -> Result<Option<SourceProxy>, String> {
    if !table_exists(connection, "proxy_config")? {
        return Ok(None);
    }
    connection
        .query_row(
            "SELECT enabled, auto_failover_enabled, max_retries, \
             streaming_first_byte_timeout, streaming_idle_timeout, non_streaming_timeout, \
             circuit_failure_threshold, circuit_success_threshold, circuit_timeout_seconds, \
             circuit_error_rate_threshold, circuit_min_requests \
             FROM proxy_config WHERE app_type = 'claude'",
            [],
            |row| {
                Ok(SourceProxy {
                    enabled: row.get::<_, i64>(0)? != 0,
                    auto_failover_enabled: row.get::<_, i64>(1)? != 0,
                    max_retries: row.get(2)?,
                    streaming_first_byte_timeout: row.get(3)?,
                    streaming_idle_timeout: row.get(4)?,
                    non_streaming_timeout: row.get(5)?,
                    circuit_failure_threshold: row.get(6)?,
                    circuit_success_threshold: row.get(7)?,
                    circuit_timeout_seconds: row.get(8)?,
                    circuit_error_rate_threshold: row.get(9)?,
                    circuit_min_requests: row.get(10)?,
                })
            },
        )
        .optional()
        .map_err(|error| format!("无法读取 Claude 代理策略: {error}"))
}

fn policy_revision(state: &LocalState) -> Result<String, String> {
    let text = crate::config_store::read_optional(&failover::path(state.root()))
        .map_err(|_| "Claude 故障转移策略不可读".to_string())?;
    Ok(asb_switch::sha256_hex(&text.unwrap_or_default()))
}

fn column_exists(connection: &Connection, table: &str, column: &str) -> Result<bool, String> {
    if !table_exists(connection, table)? {
        return Ok(false);
    }
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| format!("无法读取来源表结构: {error}"))?;
    let names = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| format!("无法读取来源表结构: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("无法读取来源表结构: {error}"))?;
    Ok(names.iter().any(|name| name == column))
}

#[cfg(test)]
mod tests;
