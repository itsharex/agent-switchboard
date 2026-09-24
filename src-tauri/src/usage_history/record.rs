use super::ledger::{
    mutate, normalize_text, normalized_timestamp, prune_official, prune_providers,
    validate_optional_number, OfficialHistoryPoint, ProviderHistoryPoint,
};
use crate::local_state::LocalState;
use asb_core::contracts::{CodexOfficialQuota, ProviderProfile, UsageSummary};

/// Records every normalized reading in one successful provider network query.
/// The caller treats persistence errors as warnings so a fresh query result
/// remains visible even when this optional local history cannot be updated.
pub(crate) fn record_provider(
    state: &LocalState,
    profile: &ProviderProfile,
    summary: &UsageSummary,
) -> Result<(), String> {
    let query = profile
        .usage_query
        .as_ref()
        .ok_or_else(|| "该供应商尚未配置用量查询".to_string())?;
    let digest = crate::usage_cache::query_digest(query)?;
    let at = normalized_timestamp(&summary.at)?;
    let points = summary
        .readings
        .iter()
        .filter(|reading| reading.is_valid != Some(false))
        .map(|reading| {
            validate_optional_number(reading.remaining)?;
            validate_optional_number(reading.used)?;
            validate_optional_number(reading.total)?;
            if reading.remaining.is_none() && reading.used.is_none() && reading.total.is_none() {
                return Err("供应商用量读数为空".to_string());
            }
            Ok(ProviderHistoryPoint {
                profile_id: profile.id.clone(),
                query_digest: digest.clone(),
                at: at.clone(),
                plan_name: normalize_text(reading.plan_name.as_deref()),
                unit: normalize_text(reading.unit.as_deref()),
                remaining: reading.remaining,
                used: reading.used,
                total: reading.total,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    mutate(state, move |ledger| {
        ledger.providers.extend(points);
        prune_providers(&mut ledger.providers);
    })
}

/// Records every server-declared official quota window from a successful
/// official read. `account_changed` comes from the existing comparison
/// baseline and prevents points belonging to two detected accounts from
/// sharing one trend.
pub(crate) fn record_official(
    state: &LocalState,
    quota: &CodexOfficialQuota,
    reset_history: bool,
) -> Result<(), String> {
    let at = quota
        .at
        .as_deref()
        .ok_or_else(|| "官方额度读取时间缺失".to_string())
        .and_then(normalized_timestamp)?;
    let points = quota
        .windows
        .iter()
        .map(|window| {
            let point = OfficialHistoryPoint {
                at: at.clone(),
                window_label: window.label.trim().to_string(),
                used_percent: window.used_percent,
                resets_at: window
                    .resets_at
                    .as_deref()
                    .map(normalized_timestamp)
                    .transpose()?,
            };
            point.validate()?;
            Ok(point)
        })
        .collect::<Result<Vec<_>, String>>()?;

    mutate(state, move |ledger| {
        if reset_history {
            ledger.official.clear();
        }
        ledger.official.extend(points);
        prune_official(&mut ledger.official);
    })
}

/// Drops every historical provider reading for one profile after an edit or
/// deletion. Official quota history is account-scoped and remains separate.
pub(crate) fn invalidate_provider(state: &LocalState, profile_id: &str) -> Result<(), String> {
    mutate(state, |ledger| {
        ledger
            .providers
            .retain(|point| point.profile_id != profile_id);
    })
}
