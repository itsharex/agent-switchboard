use serde_json::Value as Json;

use crate::adapter::{AdapterError, OverlayEntry};
use crate::contracts::{AppKind, ChangeKind, KeyChange, SwitchPlan, SwitchPreview};
use crate::redact::redact;

use crate::adapter::claude::document::{get, parse, scalar_repr, validate_path};
use crate::adapter::claude::overlay::{overlay, ENV_MODEL_KEY};

pub(crate) fn preview(
    current: &str,
    plan: &SwitchPlan,
    backup_dir: &str,
) -> Result<SwitchPreview, AdapterError> {
    let mut warnings = Vec::new();
    let root = parse(current)?;
    if plan.profile.model.is_some() && get(&root, ENV_MODEL_KEY).is_some() {
        warnings.push(crate::contracts::LocalizedMessage::new(
            "warnings.claude.modelOverridden",
            serde_json::json!({}),
            "settings.json 的 env.ANTHROPIC_MODEL 会覆盖 model；切换将移除该键",
        ));
    }
    let mut entries = overlay(plan);
    entries.extend(super::native::entries(current, plan)?);
    let mut preview = preview_entries_from_root(root, entries, warnings, backup_dir)?;
    crate::claude_common::validate_scopes(
        &plan.client_settings.claude_extra,
        &plan.profile.claude_fragment,
    )
    .map_err(|message| AdapterError {
        message,
        line: None,
    })?;
    let fragment_changes = |result: Result<Vec<KeyChange>, String>| {
        result.map_err(|message| AdapterError {
            message,
            line: None,
        })
    };
    preview
        .changes
        .extend(fragment_changes(crate::claude_common::changes(
            current,
            &plan.client_settings.claude_extra,
        ))?);
    preview
        .changes
        .extend(fragment_changes(crate::claude_common::changes_profile(
            current,
            &plan.profile.claude_fragment,
        ))?);
    Ok(preview)
}

fn preview_entries_from_root(
    root: Json,
    entries: Vec<(String, OverlayEntry)>,
    warnings: Vec<crate::contracts::LocalizedMessage>,
    backup_dir: &str,
) -> Result<SwitchPreview, AdapterError> {
    for (key, entry) in &entries {
        if !matches!(entry, OverlayEntry::Leave) {
            validate_path(&root, key)?;
        }
    }

    let mut changes = Vec::new();
    for (key, entry) in entries {
        let existing = get(&root, &key).and_then(scalar_repr);
        match entry {
            OverlayEntry::Set(value) => {
                let after = value.display();
                if existing.as_deref() != Some(after.as_str()) {
                    changes.push(KeyChange {
                        key: key.clone(),
                        kind: ChangeKind::Set,
                        before: existing.map(|b| redact(&key, &b)),
                        after: Some(redact(&key, &after)),
                    });
                }
            }
            // Absent overlay fields for the shared `env` object only leave
            // keys that this overlay never claims.
            OverlayEntry::Leave => {}
            // Owned keys are removed when present.
            OverlayEntry::RemoveIfPresent => {
                if let Some(before) = existing {
                    changes.push(KeyChange {
                        key: key.clone(),
                        kind: ChangeKind::Remove,
                        before: Some(redact(&key, &before)),
                        after: None,
                    });
                }
            }
            OverlayEntry::RemoveTableIfEmpty => {}
        }
    }

    Ok(SwitchPreview {
        app: AppKind::Claude,
        target: AppKind::Claude.config_label().to_string(),
        changes,
        warnings,
        backup_dir: backup_dir.to_string(),
    })
}
