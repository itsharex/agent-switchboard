//! Retired configuration conversion at the explicit switch boundary.
//! Normal adapters and restore never recognize these provider identities.
use super::{preview::plan_rejected, SwitchError};
use asb_core::{adapter, AppKind, ChangeKind, KeyChange, SwitchPlan, SwitchPreview};

const OLD_FIELDS: &[(&str, &[&str])] = &[
    (
        "agent_switchboard",
        &[
            "name",
            "base_url",
            "wire_api",
            "requires_openai_auth",
            "supports_websockets",
            "env_key",
            "experimental_bearer_token",
        ],
    ),
    (
        "OpenAi",
        &["name", "base_url", "wire_api", "experimental_bearer_token"],
    ),
];

pub(super) fn candidate(
    current: &str,
    plan: &SwitchPlan,
    backup_dir: &str,
) -> Result<(SwitchPreview, String), SwitchError> {
    let (input, removed) = if plan.app() == AppKind::Codex {
        convert_retired_codex_fields(current)?
    } else {
        (current.to_string(), Vec::new())
    };
    let mut preview = adapter::preview(&input, plan, backup_dir).map_err(plan_rejected)?;
    if !removed.is_empty() {
        preview.changes.extend(removed);
        preview.warnings.push(
            "确认切换时将一次性移除本产品旧 provider 字段，原配置会备份；登录与历史保持不变".into(),
        );
    }
    let rendered = adapter::render(&input, plan).map_err(plan_rejected)?;
    Ok((preview, rendered))
}

fn convert_retired_codex_fields(current: &str) -> Result<(String, Vec<KeyChange>), SwitchError> {
    adapter::validate_syntax(AppKind::Codex, current).map_err(plan_rejected)?;
    let mut doc = current
        .parse::<toml_edit::DocumentMut>()
        .expect("syntax validated");
    let mut changes = Vec::new();
    if let Some(providers) = doc
        .get_mut("model_providers")
        .and_then(|v| v.as_table_like_mut())
    {
        for (id, fields) in OLD_FIELDS {
            if let Some(table) = providers.get_mut(id).and_then(|v| v.as_table_like_mut()) {
                for field in *fields {
                    if let Some(value) = table.remove(field) {
                        changes.push(KeyChange {
                            key: format!("model_providers.{id}.{field}"),
                            kind: ChangeKind::Remove,
                            before: Some(if *field == "experimental_bearer_token" {
                                asb_core::redact::REDACTED.to_string()
                            } else {
                                asb_core::redact::redact(
                                    &format!("model_providers.{id}.{field}"),
                                    &value.to_string(),
                                )
                            }),
                            after: None,
                        });
                    }
                }
                if table.is_empty() {
                    providers.remove(id);
                }
            }
        }
        if providers.is_empty() {
            doc.remove("model_providers");
        }
    }
    if doc.remove("experimental_bearer_token").is_some() {
        changes.push(KeyChange {
            key: "experimental_bearer_token".into(),
            kind: ChangeKind::Remove,
            before: Some(asb_core::redact::REDACTED.into()),
            after: None,
        });
    }
    Ok((
        if changes.is_empty() {
            current.to_string()
        } else {
            doc.to_string()
        },
        changes,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversion_is_idempotent_and_preserves_unowned_fields() {
        let original = "[model_providers.agent_switchboard]\nbase_url = \"https://old.example\"\nrequest_timeout_ms = 1000\n[model_providers.OpenAi]\nexperimental_bearer_token = \"secret\"\n[model_providers.other]\nbase_url = \"https://keep.example\"\n";
        let (converted, changes) = convert_retired_codex_fields(original).unwrap();
        assert!(converted.contains("request_timeout_ms = 1000"));
        assert!(converted.contains("https://keep.example"));
        assert!(!converted.contains("https://old.example"));
        assert!(!converted.contains("secret"));
        assert!(!format!("{changes:?}").contains("secret"));
        let (again, changes) = convert_retired_codex_fields(&converted).unwrap();
        assert_eq!(again, converted);
        assert!(changes.is_empty());
    }
}
