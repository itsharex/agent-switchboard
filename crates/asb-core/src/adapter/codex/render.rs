use crate::adapter::{AdapterError, OverlayEntry};
use crate::contracts::{SettingsValues, SwitchPlan};

use crate::adapter::codex::document::{parse, remove_empty_table_path, remove_path, set_path};
use crate::adapter::codex::overlay::{client_settings_overlay, overlay};

pub(crate) fn render(current: &str, plan: &SwitchPlan) -> Result<String, AdapterError> {
    super::validate_projection(plan)?;
    render_entries(current, overlay(plan))
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

pub(crate) fn render_client_settings(
    client_settings: &SettingsValues,
) -> Result<String, AdapterError> {
    let rendered = render_entries("", client_settings_overlay(client_settings))?;
    Ok(if rendered.trim().is_empty() {
        "# 所有客户端设置均为自动\n".to_string()
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
