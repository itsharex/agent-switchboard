use crate::adapter::{AdapterError, OverlayEntry};
use crate::contracts::{ChangeKind, KeyChange, SwitchPlan, SwitchPreview};
use crate::redact::redact;
use crate::AppKind;

use crate::adapter::codex::document::{item_at, item_repr, parse, validate_path};
use crate::adapter::codex::overlay::overlay;

pub(crate) fn preview(
    current: &str,
    plan: &SwitchPlan,
    backup_dir: &str,
) -> Result<SwitchPreview, AdapterError> {
    preview_entries(current, overlay(plan), backup_dir)
}

pub(crate) fn preview_entries(
    current: &str,
    entries: Vec<(String, OverlayEntry)>,
    backup_dir: &str,
) -> Result<SwitchPreview, AdapterError> {
    let doc = parse(current)?;
    for (key, entry) in &entries {
        if !matches!(
            entry,
            OverlayEntry::Leave | OverlayEntry::RemoveTableIfEmpty
        ) {
            validate_path(&doc, key)?;
        }
    }
    let mut changes = Vec::new();
    for (key, entry) in entries {
        let existing = item_at(&doc, &key).and_then(item_repr);
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
            OverlayEntry::Leave => {}
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
        app: AppKind::Codex,
        target: AppKind::Codex.config_label().to_string(),
        changes,
        warnings: vec![],
        backup_dir: backup_dir.to_string(),
    })
}
