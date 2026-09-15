//! Codex-only CC Switch failover import: the source's ordered Codex queue and
//! its per-app proxy policy become a proposed `CodexGatewayPolicy`. The scan
//! is strictly read-only and only proposes; applying the proposal goes
//! through the existing Codex policy preview/commit transaction, so this
//! module never writes a policy or a client file. Source health rows and the
//! source's current-provider marker are ignored: health is computed live and
//! the active route is observed from the real client files.

use super::db::{column_exists, open_read_only, table_exists};
use crate::gateway::codex::policy::{self, CodexGatewayPolicy};
use crate::local_state::LocalState;
use asb_core::ccswitch::{self, CcSwitchProviderDraft, CcSwitchRow};
use asb_core::contracts::{CodexProviderFile, CodexUpstream};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexFailoverQueueMember {
    pub source_name: String,
    pub endpoint: Option<String>,
    pub upstream: Option<CodexUpstream>,
    pub matched_profile_id: Option<String>,
    pub matched_profile_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexFailoverSourceScan {
    pub found: bool,
    pub source_revision: String,
    pub policy_revision: String,
    pub members: Vec<CodexFailoverQueueMember>,
    pub proposal: CodexGatewayPolicy,
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

/// Read-only scan: the ordered source queue with local match states plus the
/// proposed policy, seeded from the current local policy so unmapped local
/// values survive. The proposal is validated here so the caller only ever
/// receives a policy the preview transaction will accept.
pub(crate) fn scan(source: &Path, state: &LocalState) -> Result<CodexFailoverSourceScan, String> {
    let connection = open_read_only(source)?;
    let queue = read_queue(&connection)?;
    let proxy = read_proxy(&connection)?;
    let source_revision = revision(&queue, proxy.as_ref());
    let (mut proposal, policy_revision) = policy::load(state.root())
        .map_err(|error| format!("本机 Codex 网关策略不可读：{error}"))?;
    let snapshots = state
        .configuration()
        .codex_provider_snapshots()
        .map_err(|error| format!("本机 Codex 供应商不可读：{error}"))?
        .into_iter()
        .map(|(file, _)| file)
        .collect::<Vec<_>>();
    let mut warnings = Vec::new();
    let members = queue
        .iter()
        .map(|row| member_for(row, &snapshots, &mut warnings))
        .collect::<Vec<_>>();
    proposal.provider_ids = queue_ids(&members, &mut warnings);
    if let Some(proxy) = &proxy {
        apply_proxy(&mut proposal, proxy, &mut warnings);
    }
    reconcile(&mut proposal, &mut warnings);
    proposal
        .validate()
        .map_err(|error| format!("来源策略无法转换为本机 Codex 网关策略：{error}"))?;
    Ok(CodexFailoverSourceScan {
        found: !members.is_empty() || proxy.is_some(),
        source_revision,
        policy_revision,
        members,
        proposal,
        warnings,
    })
}

fn member_for(
    row: &QueueRow,
    snapshots: &[CodexProviderFile],
    warnings: &mut Vec<String>,
) -> CodexFailoverQueueMember {
    let source = CcSwitchRow {
        id: row.id.clone(),
        app_type: "codex".into(),
        name: row.name.clone(),
        settings_config: row.settings_config.clone(),
        website_url: None,
        notes: None,
        display: None,
        meta: row.meta.clone(),
    };
    let mut member = CodexFailoverQueueMember {
        source_name: row.name.clone(),
        endpoint: None,
        upstream: None,
        matched_profile_id: None,
        matched_profile_name: None,
    };
    let seed = match ccswitch::map_row(&source).map(|proposal| proposal.draft) {
        Ok(CcSwitchProviderDraft::Codex(seed)) => seed,
        Ok(CcSwitchProviderDraft::CodexOfficial(_)) => {
            warnings.push(format!(
                "未加入队列: {}（官方登录不参与故障转移队列）",
                row.name
            ));
            return member;
        }
        Ok(CcSwitchProviderDraft::Claude(_)) => return member,
        Err(reason) => {
            warnings.push(format!("未导入: {}（{}）", row.name, reason.reason));
            return member;
        }
    };
    member.endpoint = Some(seed.endpoint.0.clone());
    member.upstream = Some(seed.upstream);
    match snapshots.iter().find(|file| {
        file.profile.endpoint == seed.endpoint && file.profile.upstream == seed.upstream
    }) {
        Some(file) => {
            member.matched_profile_id = Some(file.profile.id.clone());
            member.matched_profile_name = Some(file.profile.name.clone());
        }
        None => warnings.push(format!(
            "未加入队列: {}（本地没有相同路由的档案，请先导入该供应商）",
            row.name
        )),
    }
    member
}

/// Source order, one local profile at most once: two source rows routing to
/// the same local profile would otherwise make the queue retry itself.
fn queue_ids(members: &[CodexFailoverQueueMember], warnings: &mut Vec<String>) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    for member in members {
        let Some(id) = &member.matched_profile_id else {
            continue;
        };
        if ids.contains(id) {
            warnings.push(format!(
                "未重复加入队列: {}（与前面的来源项映射到同一本地档案）",
                member.source_name
            ));
            continue;
        }
        ids.push(id.clone());
    }
    ids
}

fn apply_proxy(policy: &mut CodexGatewayPolicy, proxy: &SourceProxy, warnings: &mut Vec<String>) {
    policy.takeover = proxy.enabled;
    policy.enabled = proxy.auto_failover_enabled;
    if (0..=10).contains(&proxy.max_retries) {
        policy.max_retries = proxy.max_retries as u32;
    } else {
        warnings.push(format!(
            "来源重试次数 {} 超出本机允许范围 0–10，保留本地值",
            proxy.max_retries
        ));
    }
    let traffic = &mut policy.traffic;
    // Source timeouts share the local zero-means-unlimited semantics, so zero
    // imports as-is; the header phase has no source counterpart and stays local.
    for (target, source, name) in [
        (
            &mut traffic.first_byte_timeout_seconds,
            proxy.streaming_first_byte_timeout,
            "流式首包超时",
        ),
        (
            &mut traffic.idle_timeout_seconds,
            proxy.streaming_idle_timeout,
            "流式空闲超时",
        ),
        (
            &mut traffic.non_streaming_timeout_seconds,
            proxy.non_streaming_timeout,
            "非流式总超时",
        ),
    ] {
        take(target, source, 0, 86_400, name, warnings);
    }
    take(
        &mut traffic.failure_threshold,
        proxy.circuit_failure_threshold,
        1,
        100,
        "熔断失败阈值",
        warnings,
    );
    take(
        &mut traffic.success_threshold,
        proxy.circuit_success_threshold,
        1,
        100,
        "熔断恢复阈值",
        warnings,
    );
    take(
        &mut traffic.cooldown_seconds,
        proxy.circuit_timeout_seconds,
        1,
        86_400,
        "熔断冷却时间",
        warnings,
    );
    take(
        &mut traffic.min_requests,
        proxy.circuit_min_requests,
        1,
        10_000,
        "错误率最小请求数",
        warnings,
    );
    // The source stores an error-rate fraction (0..1); the local contract
    // stores a percent. A zero fraction has no local expression.
    let percent = (proxy.circuit_error_rate_threshold * 100.0).round() as i64;
    if (1..=100).contains(&percent) {
        traffic.error_rate_percent = percent as u32;
    } else {
        warnings.push(format!(
            "来源熔断错误率 {} 无法换算为本机百分比档位，保留本地值",
            proxy.circuit_error_rate_threshold
        ));
    }
}

fn take(target: &mut u32, source: i64, min: u32, max: u32, name: &str, warnings: &mut Vec<String>) {
    if source >= i64::from(min) && source <= i64::from(max) {
        *target = source as u32;
    } else {
        warnings.push(format!(
            "来源{name} {source} 超出本机允许范围 {min}–{max}，保留本地值"
        ));
    }
}

/// Failover is only meaningful behind the takeover with a non-empty queue;
/// a source that enables it without either keeps the proposal off, loudly.
fn reconcile(policy: &mut CodexGatewayPolicy, warnings: &mut Vec<String>) {
    if !policy.enabled {
        return;
    }
    if !policy.takeover {
        policy.enabled = false;
        warnings.push("来源已开启自动故障转移但未启用接管，提案保持 Failover 关闭".into());
    } else if policy.provider_ids.is_empty() {
        policy.enabled = false;
        warnings.push("来源已开启自动故障转移但本地队列为空，提案保持 Failover 关闭".into());
    }
}

fn revision(queue: &[QueueRow], proxy: Option<&SourceProxy>) -> String {
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
    asb_switch::sha256_hex(
        &serde_json::json!({ "queue": rows, "proxy": proxy.map(proxy_json) }).to_string(),
    )
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
             WHERE app_type = 'codex' AND in_failover_queue = 1 \
             ORDER BY COALESCE(sort_index, 999999), id ASC",
        )
        .map_err(|error| format!("无法读取 Codex 故障转移队列: {error}"))?;
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
        .map_err(|error| format!("无法读取 Codex 故障转移队列: {error}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Codex 故障转移队列格式无效: {error}"));
    rows
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
             FROM proxy_config WHERE app_type = 'codex'",
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
        .map_err(|error| format!("无法读取 Codex 代理策略: {error}"))
}

#[cfg(test)]
mod tests;
