use crate::adapter::{AdapterError, OverlayEntry};
use crate::contracts::{CommonSettings, SwitchPlan};

use crate::adapter::codex::document::{parse, remove_empty_table_path, remove_path, set_path};
use crate::adapter::codex::overlay::{common_overlay, overlay};

pub(crate) fn render(current: &str, plan: &SwitchPlan) -> Result<String, AdapterError> {
    render_entries(current, overlay(plan))
}

pub(crate) fn render_gateway_base_url(
    current: &str,
    base_url: &str,
) -> Result<String, AdapterError> {
    render_entries(
        current,
        vec![(
            "openai_base_url".to_string(),
            OverlayEntry::Set(crate::contracts::ConfigValue::Str(base_url.to_string())),
        )],
    )
}

pub(crate) fn render_common_settings(common: &CommonSettings) -> Result<String, AdapterError> {
    let rendered = render_entries("", common_overlay(common))?;
    Ok(if rendered.trim().is_empty() {
        "# 所有通用设置均为自动\n".to_string()
    } else {
        rendered
    })
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
    Ok(doc.to_string())
}
