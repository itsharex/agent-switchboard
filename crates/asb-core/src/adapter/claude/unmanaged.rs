//! Deep-reset pruning for Claude `settings.json`.
//!
//! Adapter overlays only ever touch keys in the ownership directory; this
//! module is the single inverse operation: it deletes every leaf no ASB
//! module claims so the deep client reset can converge the real file onto the
//! configuration the interface models. Preserved content: directory scalars
//! (provider and client), native cloud namespaces, and the manifest-claimed
//! extra-configuration paths.

use std::collections::BTreeSet;

use serde_json::{Map, Value as Json};

use crate::adapter::AdapterError;
use crate::contracts::AppKind;
use crate::ownership::is_owned;

/// Removes every unowned leaf and prunes the objects it empties. Returns the
/// input unchanged when nothing would be removed, so a no-op reset never
/// rewrites the live file.
pub(crate) fn remove_unmanaged(text: &str) -> Result<String, AdapterError> {
    if text.trim().is_empty() {
        return Ok(text.to_string());
    }
    let mut root = super::document::parse(text)?;
    let native = super::native::owned_paths(&root)?;
    let common = crate::claude_common::owned_dotted_paths(&root)
        .map_err(|message| AdapterError { message, line: None })?;
    let mut removed = false;
    prune_object(&mut root, String::new(), &native, &common, &mut removed);
    if !removed {
        return Ok(text.to_string());
    }
    serde_json::to_string_pretty(&root).map_err(|_| AdapterError {
        message: "Claude 配置无法编码".to_string(),
        line: None,
    })
}

fn preserved(path: &str, native: &BTreeSet<String>, common: &BTreeSet<String>) -> bool {
    is_owned(AppKind::Claude, path)
        || native.contains(path)
        || common.contains(path)
        || common
            .iter()
            .any(|claimed| path.strip_prefix(claimed).is_some_and(|rest| rest.starts_with('.')))
}

fn prune_object(
    value: &mut Json,
    prefix: String,
    native: &BTreeSet<String>,
    common: &BTreeSet<String>,
    removed: &mut bool,
) {
    let Some(map) = value.as_object_mut() else {
        return;
    };
    let keys: Vec<String> = map.keys().cloned().collect();
    for key in keys {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        let keep = preserved(&path, native, common);
        let mut emptied = false;
        if map.get(&key).is_some_and(Json::is_object) {
            let child = map.get_mut(&key).expect("key snapshotted above");
            prune_object(child, path, native, common, removed);
            emptied = child.as_object().is_some_and(Map::is_empty);
        } else if !keep {
            map.remove(&key);
            *removed = true;
            continue;
        }
        if emptied && !keep {
            map.remove(&key);
            *removed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::claude::state;

    #[test]
    fn host_keys_are_removed_and_empty_objects_pruned() {
        let text = r#"{
  "spinnerTipsEnabled": true,
  "statusLine": { "type": "command" },
  "custom": { "enabled": true },
  "env": {
    "ASB_CLAUDE_COMMON_KEYS": "[\"/custom/enabled\"]",
    "MY_TOOL_TOKEN": "secret-value",
    "UNLISTED_VAR": "y"
  }
}"#;
        let rendered = remove_unmanaged(text).expect("prune");
        assert!(rendered.contains("spinnerTipsEnabled"));
        assert!(rendered.contains("custom"));
        assert!(rendered.contains("ASB_CLAUDE_COMMON_KEYS"));
        assert!(!rendered.contains("MY_TOOL_TOKEN"));
        assert!(!rendered.contains("UNLISTED_VAR"));
        assert!(!rendered.contains("statusLine"));
    }

    #[test]
    fn claimed_extra_paths_survive_as_subtrees() {
        let text = r#"{
  "custom": { "enabled": true, "extra": 1 },
  "env": { "ASB_CLAUDE_COMMON_KEYS": "[\"/custom/enabled\"]" }
}"#;
        let rendered = remove_unmanaged(text).expect("prune");
        assert!(rendered.contains("\"enabled\": true"));
        assert!(!rendered.contains("\"extra\""));
    }

    #[test]
    fn no_removals_return_text_unchanged() {
        let text = "{\n  \"model\": \"claude-x\"\n}\n";
        assert_eq!(remove_unmanaged(text).expect("prune"), text);
    }

    #[test]
    fn empty_text_passes_through() {
        assert_eq!(remove_unmanaged("").expect("prune"), "");
    }

    #[test]
    fn full_diff_lists_host_removals_that_owned_diff_hides() {
        let live = r#"{"spinnerTipsEnabled": true, "statusLine": {"type": "command"}}"#;
        let candidate = r#"{"spinnerTipsEnabled": true}"#;
        assert!(state::owned_diff(candidate, live).expect("owned diff").is_empty());
        let changes = state::full_diff(candidate, live).expect("full diff");
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].key, "statusLine.type");
    }
}
