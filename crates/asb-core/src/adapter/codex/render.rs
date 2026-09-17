use crate::adapter::{AdapterError, OverlayEntry};
use crate::contracts::{SettingsValues, SwitchPlan};

use crate::adapter::codex::common_fragment::{merge_fragment, remove_applied_fragment, validate_fragment};
use crate::adapter::codex::document::{parse, remove_empty_table_path, remove_path, set_path};
use crate::adapter::codex::overlay::{client_settings_overlay, overlay};

pub(crate) fn render(current: &str, plan: &SwitchPlan) -> Result<String, AdapterError> {
    super::validate_projection(plan)?;
    let rendered = render_entries(current, overlay(plan))?;
    match plan.codex_common_fragment() {
        None => Ok(rendered),
        Some(fragment) => {
            // 存储层可能被手工改动，投影边界处再做一次 fail-closed 校验。
            validate_fragment(&fragment.text)?;
            if fragment.enabled {
                merge_fragment(&rendered, &fragment.text)
            } else {
                remove_applied_fragment(&rendered, &fragment.text)
            }
        }
    }
}

pub(crate) fn render_gateway_base_url(
    current: &str,
    base_url: &str,
) -> Result<String, AdapterError> {
    if !super::is_gateway_base_url(base_url) {
        return Err(AdapterError {
            message: "Codex 网关地址必须是本机带路由凭证的入口".into(),
            line: None,
        });
    }
    render_entries(
        current,
        vec![(
            crate::ownership::CODEX_PROVIDER_BASE_URL_KEY.to_string(),
            OverlayEntry::Set(crate::contracts::ConfigValue::Str(base_url.to_string())),
        )],
    )
}

pub(crate) fn render_client_settings_into_file(
    current: &str,
    client_settings: &SettingsValues,
) -> Result<String, AdapterError> {
    render_entries(current, client_settings_overlay(client_settings))
}


pub(crate) fn render_entries(
    current: &str,
    entries: Vec<(String, OverlayEntry)>,
) -> Result<String, AdapterError> {
    let mut doc = parse(current)?;
    for (key, entry) in entries {
        match entry {
            OverlayEntry::Set(value) => set_path(&mut doc, &key, value)?,
            OverlayEntry::Leave => {}
            OverlayEntry::RemoveIfPresent => {
                remove_path(&mut doc, &key)?;
            }
            OverlayEntry::RemoveTableIfEmpty => {
                remove_empty_table_path(&mut doc, &key)?;
            }
        }
    }
    // A retired ASB-owned alias is cleaned by every Codex configuration write,
    // not only by the subagent settings form.
    super::subagents::remove_retired_keys(&mut doc)?;
    Ok(doc.to_string())
}
