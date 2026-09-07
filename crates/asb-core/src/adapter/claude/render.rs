use crate::adapter::{AdapterError, OverlayEntry};
use crate::contracts::{CommonSettings, SwitchPlan};

use crate::adapter::claude::document::{parse, remove, set};
use crate::adapter::claude::overlay::{common_overlay, overlay};

pub(crate) fn render(current: &str, plan: &SwitchPlan) -> Result<String, AdapterError> {
    render_entries(current, overlay(plan))
}

pub(crate) fn render_common_settings(common: &CommonSettings) -> Result<String, AdapterError> {
    render_entries("{}", common_overlay(common))
}

pub(crate) fn render_entries(
    current: &str,
    entries: Vec<(String, OverlayEntry)>,
) -> Result<String, AdapterError> {
    let mut root = parse(current)?;
    for (key, entry) in entries {
        match entry {
            OverlayEntry::Set(value) => set(&mut root, &key, value)?,
            OverlayEntry::Leave => {}
            OverlayEntry::RemoveIfPresent => {
                remove(&mut root, &key)?;
            }
            OverlayEntry::RemoveTableIfEmpty => {}
        }
    }
    serde_json::to_string_pretty(&root).map_err(|_| AdapterError {
        message: "配置序列化失败".to_string(),
        line: None,
    })
}
