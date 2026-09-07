use super::ledger::{is_within_retention, load, ProviderHistoryPoint, ProviderSeriesKey};
use crate::local_state::LocalState;
use asb_core::contracts::{
    ProviderProfile, UsageHistoryMetric, UsageHistoryPoint, UsageHistorySeries,
};
use asb_switch::sha256_hex;
use std::collections::BTreeMap;

/// Resolves provider history only for the current profile and its current
/// query digest. Old query shapes cannot surface as a current trend.
pub(crate) fn provider_series(
    state: &LocalState,
    profile: &ProviderProfile,
) -> Result<Vec<UsageHistorySeries>, String> {
    let Some(query) = profile.usage_query.as_ref() else {
        return Ok(Vec::new());
    };
    let digest = crate::usage_cache::query_digest(query)?;
    let Some(ledger) = load(state)? else {
        return Ok(Vec::new());
    };
    let points = ledger
        .providers
        .into_iter()
        .filter(|point| {
            point.profile_id == profile.id
                && point.query_digest == digest
                && is_within_retention(&point.at)
        })
        .collect::<Vec<_>>();
    Ok(provider_series_from_points(&profile.id, &digest, points))
}

/// Returns account-safe official quota series. Account markers are only used
/// by the reset baseline; they never enter this ledger or the renderer data.
pub(crate) fn official_series(state: &LocalState) -> Result<Vec<UsageHistorySeries>, String> {
    let Some(ledger) = load(state)? else {
        return Ok(Vec::new());
    };
    let mut grouped = BTreeMap::<String, Vec<UsageHistoryPoint>>::new();
    for point in ledger
        .official
        .into_iter()
        .filter(|point| is_within_retention(&point.at))
    {
        grouped
            .entry(point.window_label)
            .or_default()
            .push(UsageHistoryPoint {
                at: point.at,
                value: point.used_percent,
            });
    }
    Ok(grouped
        .into_iter()
        .map(|(label, mut points)| {
            sort_points(&mut points);
            UsageHistorySeries {
                id: series_id("official", &[&label]),
                label,
                unit: Some("%".to_string()),
                metric: UsageHistoryMetric::UsedPercent,
                points,
            }
        })
        .collect())
}

fn provider_series_from_points(
    profile_id: &str,
    digest: &str,
    history: Vec<ProviderHistoryPoint>,
) -> Vec<UsageHistorySeries> {
    let mut grouped = BTreeMap::<ProviderSeriesKey, Vec<UsageHistoryPoint>>::new();
    for point in history {
        let used = point
            .used
            .or_else(|| derived_used(point.total, point.remaining));
        push_provider_series_point(
            &mut grouped,
            &point,
            UsageHistoryMetric::Remaining,
            point.remaining,
        );
        push_provider_series_point(&mut grouped, &point, UsageHistoryMetric::Used, used);
    }

    grouped
        .into_iter()
        .map(|(key, mut points)| {
            sort_points(&mut points);
            let plan_label = key.plan_name.as_deref().unwrap_or("默认方案");
            UsageHistorySeries {
                id: series_id(
                    "provider",
                    &[
                        profile_id,
                        digest,
                        plan_label,
                        key.unit.as_deref().unwrap_or(""),
                        metric_key(key.metric),
                    ],
                ),
                label: provider_series_label(plan_label, key.metric),
                unit: key.unit,
                metric: key.metric,
                points,
            }
        })
        .collect()
}

fn push_provider_series_point(
    grouped: &mut BTreeMap<ProviderSeriesKey, Vec<UsageHistoryPoint>>,
    point: &ProviderHistoryPoint,
    metric: UsageHistoryMetric,
    value: Option<f64>,
) {
    let Some(value) = value.filter(|value| value.is_finite()) else {
        return;
    };
    grouped
        .entry(ProviderSeriesKey {
            plan_name: point.plan_name.clone(),
            unit: point.unit.clone(),
            metric,
        })
        .or_default()
        .push(UsageHistoryPoint {
            at: point.at.clone(),
            value,
        });
}

fn derived_used(total: Option<f64>, remaining: Option<f64>) -> Option<f64> {
    let (total, remaining) = total.zip(remaining)?;
    (total >= remaining).then_some(total - remaining)
}

fn metric_key(metric: UsageHistoryMetric) -> &'static str {
    match metric {
        UsageHistoryMetric::Remaining => "remaining",
        UsageHistoryMetric::Used => "used",
        UsageHistoryMetric::UsedPercent => "usedPercent",
    }
}

fn provider_series_label(plan_name: &str, metric: UsageHistoryMetric) -> String {
    let suffix = match metric {
        UsageHistoryMetric::Remaining => "余额",
        UsageHistoryMetric::Used => "已用",
        UsageHistoryMetric::UsedPercent => "已用比例",
    };
    format!("{plan_name}{suffix}")
}

fn series_id(scope: &str, values: &[&str]) -> String {
    let joined = values.join("\u{1f}");
    format!("{scope}-{}", &sha256_hex(&joined)[..16])
}

fn sort_points(points: &mut [UsageHistoryPoint]) {
    points.sort_by(|left, right| left.at.cmp(&right.at));
}
