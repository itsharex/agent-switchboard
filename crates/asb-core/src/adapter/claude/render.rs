use crate::adapter::{AdapterError, OverlayEntry};
use crate::contracts::{SettingsValues, SwitchPlan};

use crate::adapter::claude::document::{parse, remove, set};
use crate::adapter::claude::overlay::{client_settings_overlay, overlay};

pub(crate) fn render(current: &str, plan: &SwitchPlan) -> Result<String, AdapterError> {
    let mut entries = overlay(plan);
    entries.extend(super::native::entries(current, plan)?);
    let rendered = render_entries(current, entries)?;
    let fragment_error = |message: String| AdapterError {
        message,
        line: None,
    };
    crate::claude_common::validate_scopes(
        &plan.client_settings.claude_extra,
        &plan.profile.claude_fragment,
    )
    .map_err(fragment_error)?;
    let rendered = crate::claude_common::apply(&rendered, &plan.client_settings.claude_extra)
        .map_err(fragment_error)?;
    crate::claude_common::apply_profile(&rendered, &plan.profile.claude_fragment)
        .map_err(fragment_error)
}

pub(crate) fn render_gateway_base_url(
    current: &str,
    base_url: &str,
) -> Result<String, AdapterError> {
    render_entries(
        current,
        vec![(
            "env.ANTHROPIC_BASE_URL".to_string(),
            OverlayEntry::Set(crate::contracts::ConfigValue::Str(base_url.to_string())),
        )],
    )
}

pub(crate) fn render_client_settings_into_file(
    current: &str,
    client_settings: &SettingsValues,
) -> Result<String, AdapterError> {
    let rendered = render_entries(current, client_settings_overlay(client_settings))?;
    crate::claude_common::apply(&rendered, &client_settings.claude_extra).map_err(|message| {
        AdapterError {
            message,
            line: None,
        }
    })
}

pub(crate) fn render_client_settings(
    client_settings: &SettingsValues,
) -> Result<String, AdapterError> {
    let rendered = render_entries("{}", client_settings_overlay(client_settings))?;
    crate::claude_common::fragment(&rendered, &client_settings.claude_extra).map_err(|message| {
        AdapterError {
            message,
            line: None,
        }
    })
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
