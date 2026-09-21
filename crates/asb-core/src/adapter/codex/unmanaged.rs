//! Deep-reset pruning for Codex `config.toml`.
//!
//! Adapter overlays only ever touch keys in the ownership directory; this
//! module is the single inverse operation: it deletes every leaf no ASB
//! module claims so the deep client reset can converge the real file onto the
//! configuration the interface models. Preserved content: directory scalars
//! (provider and client), the sub-agent runtime keys the typed render removes
//! itself, and the whole-table families owned by the provider projection and
//! the extensions workspace, plus preserve-only families in the official directory.

use toml_edit::{Item, TableLike};
use std::collections::BTreeSet;

use crate::adapter::AdapterError;
use crate::contracts::{AppKind, CodexSubagentKey};
use crate::ownership::{is_owned, is_codex_extension_path};

/// Removes every unowned leaf and prunes the tables it empties. Returns the
/// input unchanged when nothing would be removed, so a no-op reset never
/// rewrites the live file.
pub(crate) fn remove_unmanaged(text: &str, fragment: &str) -> Result<String, AdapterError> {
    let common = super::common_fragment::declared_paths(fragment)?;
    if text.trim().is_empty() {
        return Ok(text.to_string());
    }
    let mut document = super::document::parse(text)?;
    let mut removed = false;
    prune_table(document.as_table_mut(), &[], &common, &mut removed);
    if !removed {
        return Ok(text.to_string());
    }
    Ok(document.to_string())
}

fn preserved(path: &[String], is_table: bool, common: &BTreeSet<Vec<String>>) -> bool {
    let key = crate::config_path::from_keys(path);
    is_owned(AppKind::Codex, &key)
        || is_codex_extension_path(&key)
        || path.first().is_some_and(|part| part == "model_providers")
        || key == "agents.max_threads"
        || CodexSubagentKey::ALL.iter().any(|field| field.path() == key)
        || crate::ownership::preserve_only_path(AppKind::Codex, path, is_table)
        || common.iter().any(|claimed| path.starts_with(claimed))
}

fn prune_table(table: &mut dyn TableLike, prefix: &[String], common: &BTreeSet<Vec<String>>, removed: &mut bool) {
    let keys: Vec<String> = table.iter().map(|(key, _)| key.to_string()).collect();
    for key in keys {
        let mut path = prefix.to_vec();
        path.push(key.clone());
        let is_table = table.get(&key).is_some_and(|item| item.as_table_like().is_some());
        if preserved(&path, is_table, common) { continue; }
        let emptied = if let Some(sub) = table.get_mut(&key).and_then(Item::as_table_like_mut) {
            prune_table(sub, &path, common, removed);
            sub.is_empty()
        } else {
            table.remove(&key);
            *removed = true;
            continue;
        };
        if emptied {
            table.remove(&key);
            *removed = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_leaves_are_removed_and_empty_tables_pruned() {
        let text = "model = \"gpt-5\"\n\n[tools]\nflag = true\n";
        let rendered = remove_unmanaged(text, "").expect("prune");
        assert!(rendered.contains("model = \"gpt-5\""));
        assert!(!rendered.contains("tools"));
        assert!(!rendered.contains("flag"));
    }

    #[test]
    fn provider_and_extension_families_survive() {
        let text = "[model_providers.custom]\nbase_url = \"https://x\"\n\n[mcp_servers.one]\ncommand = \"n\"\n\n[skills.config.a]\nenabled = true\n\n[host_thing]\nx = 1\n";
        let rendered = remove_unmanaged(text, "").expect("prune");
        assert!(rendered.contains("model_providers.custom"));
        assert!(rendered.contains("mcp_servers.one"));
        assert!(rendered.contains("skills.config"));
        assert!(!rendered.contains("host_thing"));
    }

    #[test]
    fn inline_provider_tables_survive() {
        let text = "model_providers = { custom = { base_url = \"https://x\" } }\nhost = 1\n";
        let rendered = remove_unmanaged(text, "").expect("prune");
        assert!(rendered.contains("base_url"));
        assert!(!rendered.contains("host"));
    }

    #[test]
    fn skills_config_array_of_tables_survives() {
        let text = "[[skills.config]]\npath = \"~/.claude/skills\"\n\n[[host_rules]]\nname = \"a\"\n";
        let rendered = remove_unmanaged(text, "").expect("prune");
        assert!(rendered.contains("skills.config"));
        assert!(rendered.contains("path"));
        assert!(!rendered.contains("host_rules"));
    }

    #[test]
    fn directory_scalars_survive_inside_shared_tables() {
        let text = "[tui]\nanimations = true\nuser_extra = 1\n";
        let rendered = remove_unmanaged(text, "").expect("prune");
        assert!(rendered.contains("animations"));
        assert!(!rendered.contains("user_extra"));
        assert!(rendered.contains("[tui]"));
    }

    #[test]
    fn subagent_and_provider_agents_keys_survive() {
        let text = "[agents]\nenabled = true\ndefault_subagent_model = \"m\"\nextra = 1\n";
        let rendered = remove_unmanaged(text, "").expect("prune");
        assert!(rendered.contains("enabled"));
        assert!(rendered.contains("default_subagent_model"));
        assert!(!rendered.contains("extra"));
        assert!(rendered.contains("[agents]"));
    }

    #[test]
    fn no_removals_return_text_unchanged() {
        let text = "model = \"gpt-5\"\n";
        assert_eq!(remove_unmanaged(text, "").expect("prune"), text);
    }

    #[test]
    fn empty_text_passes_through() {
        assert_eq!(remove_unmanaged("", "").expect("prune"), "");
    }
}
