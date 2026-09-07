use crate::local_state::LocalState;
use asb_core::contracts::UsageHistoryMetric;
use chrono::{DateTime, Duration, FixedOffset, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::sync::{Mutex, OnceLock};
use uuid::Uuid;

pub(super) const HISTORY_RETENTION_DAYS: i64 = 365;
pub(super) const MAX_POINTS_PER_SERIES: usize = 720;

static LEDGER_MUTATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn mutation_lock() -> &'static Mutex<()> {
    LEDGER_MUTATION_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct UsageHistoryLedger {
    pub(super) providers: Vec<ProviderHistoryPoint>,
    pub(super) official: Vec<OfficialHistoryPoint>,
}

impl UsageHistoryLedger {
    fn is_empty(&self) -> bool {
        self.providers.is_empty() && self.official.is_empty()
    }

    fn validate(&self) -> Result<(), String> {
        for point in &self.providers {
            point.validate()?;
        }
        for point in &self.official {
            point.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ProviderHistoryPoint {
    pub(super) profile_id: String,
    pub(super) query_digest: String,
    pub(super) at: String,
    pub(super) plan_name: Option<String>,
    pub(super) unit: Option<String>,
    pub(super) remaining: Option<f64>,
    pub(super) used: Option<f64>,
    pub(super) total: Option<f64>,
}

impl ProviderHistoryPoint {
    fn validate(&self) -> Result<(), String> {
        if self.profile_id.is_empty() || self.query_digest.is_empty() {
            return Err("供应商历史标识无效".to_string());
        }
        parse_timestamp(&self.at)?;
        validate_optional_number(self.remaining)?;
        validate_optional_number(self.used)?;
        validate_optional_number(self.total)?;
        if self.remaining.is_none() && self.used.is_none() && self.total.is_none() {
            return Err("供应商历史读数为空".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct OfficialHistoryPoint {
    pub(super) at: String,
    pub(super) window_label: String,
    pub(super) used_percent: f64,
    pub(super) resets_at: Option<String>,
}

impl OfficialHistoryPoint {
    pub(super) fn validate(&self) -> Result<(), String> {
        parse_timestamp(&self.at)?;
        if self.window_label.trim().is_empty()
            || !self.used_percent.is_finite()
            || !(0.0..=100.0).contains(&self.used_percent)
        {
            return Err("官方额度历史窗口无效".to_string());
        }
        if let Some(resets_at) = &self.resets_at {
            parse_timestamp(resets_at)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct ProviderSeriesKey {
    pub(super) plan_name: Option<String>,
    pub(super) unit: Option<String>,
    pub(super) metric: UsageHistoryMetric,
}

fn parse_timestamp(value: &str) -> Result<DateTime<FixedOffset>, String> {
    DateTime::parse_from_rfc3339(value).map_err(|_| "历史读取时间无效".to_string())
}

pub(super) fn normalized_timestamp(value: &str) -> Result<String, String> {
    Ok(parse_timestamp(value)?
        .with_timezone(&Utc)
        .to_rfc3339_opts(SecondsFormat::Millis, true))
}

pub(super) fn validate_optional_number(value: Option<f64>) -> Result<(), String> {
    if value.is_some_and(|value| !value.is_finite()) {
        return Err("供应商历史数值无效".to_string());
    }
    Ok(())
}

pub(super) fn normalize_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(super) fn is_within_retention(value: &str) -> bool {
    parse_timestamp(value)
        .map(|at| at.with_timezone(&Utc) >= Utc::now() - Duration::days(HISTORY_RETENTION_DAYS))
        .unwrap_or(false)
}

pub(super) fn prune_providers(points: &mut Vec<ProviderHistoryPoint>) {
    points.retain(|point| is_within_retention(&point.at));
    points.sort_by(|left, right| left.at.cmp(&right.at));
    let mut seen = BTreeMap::<(String, String, Option<String>, Option<String>), usize>::new();
    points.reverse();
    points.retain(|point| {
        let key = (
            point.profile_id.clone(),
            point.query_digest.clone(),
            point.plan_name.clone(),
            point.unit.clone(),
        );
        let count = seen.entry(key).or_default();
        *count += 1;
        *count <= MAX_POINTS_PER_SERIES
    });
    points.reverse();
}

pub(super) fn prune_official(points: &mut Vec<OfficialHistoryPoint>) {
    points.retain(|point| is_within_retention(&point.at));
    points.sort_by(|left, right| left.at.cmp(&right.at));
    let mut seen = BTreeMap::<String, usize>::new();
    points.reverse();
    points.retain(|point| {
        let count = seen.entry(point.window_label.clone()).or_default();
        *count += 1;
        *count <= MAX_POINTS_PER_SERIES
    });
    points.reverse();
}

pub(super) fn load(state: &LocalState) -> Result<Option<UsageHistoryLedger>, String> {
    let path = state.usage_history_path();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("用量历史不可读".to_string()),
    };
    let ledger: UsageHistoryLedger =
        serde_json::from_str(&text).map_err(|_| "用量历史格式无效".to_string())?;
    ledger
        .validate()
        .map_err(|_| "用量历史格式无效".to_string())?;
    Ok(Some(ledger))
}

pub(super) fn mutate(
    state: &LocalState,
    operation: impl FnOnce(&mut UsageHistoryLedger),
) -> Result<(), String> {
    let _guard = mutation_lock()
        .lock()
        .map_err(|_| "用量历史写入锁不可用".to_string())?;
    let mut ledger = load(state)?.unwrap_or_default();
    operation(&mut ledger);
    if ledger.is_empty() {
        return match fs::remove_file(state.usage_history_path()) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("无法清除用量历史".to_string()),
        };
    }
    save(state, &ledger)
}

pub(super) fn save(state: &LocalState, ledger: &UsageHistoryLedger) -> Result<(), String> {
    let content =
        serde_json::to_string_pretty(ledger).map_err(|_| "用量历史序列化失败".to_string())?;
    let path = state.usage_history_path();
    let parent = path
        .parent()
        .ok_or_else(|| "用量历史目录无效".to_string())?;
    fs::create_dir_all(parent).map_err(|_| "无法创建应用数据目录".to_string())?;
    let temporary = parent.join(format!("usage-history.{}.tmp", Uuid::new_v4()));
    fs::write(&temporary, content).map_err(|_| "无法写入用量历史临时文件".to_string())?;
    if fs::rename(&temporary, path).is_err() {
        let _ = fs::remove_file(&temporary);
        return Err("无法原子保存用量历史".to_string());
    }
    Ok(())
}
