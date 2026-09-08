use super::*;
use crate::config_store::snapshot::{read_configuration_snapshot, ConfigurationSnapshot};
use asb_core::contracts::{AppKind, ResponsesRequestMode};
use serde_json::{json, Value};

fn previous_store() -> (tempfile::TempDir, ConfigStore, ConfigurationSnapshot) {
    let directory = tempfile::tempdir().unwrap();
    let store = ConfigStore::new(directory.path().join("state"));
    for (name, mode) in [
        ("standard", ResponsesRequestMode::Standard),
        ("minimal", ResponsesRequestMode::Minimal),
    ] {
        let mut draft = super::tests::draft(AppKind::Codex, name);
        draft.responses_options.as_mut().unwrap().request_mode = mode;
        store.create_provider(draft).unwrap();
    }
    store
        .create_provider(super::tests::draft(AppKind::Claude, "native Claude"))
        .unwrap();
    let expected = read_configuration_snapshot(&store).unwrap();
    for (index, file) in expected.providers[&AppKind::Codex].iter().enumerate() {
        let mut value = serde_json::to_value(file).unwrap();
        value["responsesOptions"]["supportsWebsockets"] = json!(index == 0);
        fs::write(
            provider_path(&store, &file.id),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
    (directory, store, expected)
}

fn provider_path(store: &ConfigStore, id: &str) -> PathBuf {
    store
        .providers_dir(AppKind::Codex)
        .join(format!("{id}.json"))
}

fn backups(store: &ConfigStore) -> Vec<PathBuf> {
    fs::read_dir(&store.state_root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("configuration-before-upgrade-")
        })
        .collect()
}

#[test]
fn confirmed_transaction_material_blocks_upgrade_without_changing_its_version() {
    for name in [
        super::super::SWITCH_INTENT_FILE,
        super::super::PROFILE_PREIMAGE_FILE,
    ] {
        let (_directory, store, _) = previous_store();
        fs::write(store.configuration_dir().join(name), "fixture transaction").unwrap();
        let before = layout::fingerprint(&store.configuration_dir()).unwrap();
        assert!(store
            .upgrade_if_needed()
            .unwrap_err()
            .contains("未完成的事务"));
        assert_eq!(
            layout::fingerprint(&store.configuration_dir()).unwrap(),
            before
        );
        assert!(backups(&store).is_empty());
    }
}

#[test]
fn startup_conversion_preserves_profile_facts_and_retains_original_bytes() {
    let (_directory, store, expected) = previous_store();
    let before = layout::fingerprint(&store.configuration_dir()).unwrap();
    assert!(store.list_providers().is_err());
    assert_eq!(
        layout::fingerprint(&store.configuration_dir()).unwrap(),
        before
    );
    assert!(store.upgrade_if_needed().unwrap());
    assert_eq!(read_configuration_snapshot(&store).unwrap(), expected);
    let retained = backups(&store);
    assert_eq!(retained.len(), 1);
    assert_eq!(layout::fingerprint(&retained[0]).unwrap(), before);
    assert!(!store.upgrade_if_needed().unwrap());
    assert_eq!(backups(&store), retained);
    for file in &expected.providers[&AppKind::Codex] {
        assert!(!fs::read_to_string(provider_path(&store, &file.id))
            .unwrap()
            .contains("supportsWebsockets"));
    }
}

#[test]
fn invalid_mixed_or_unknown_material_never_replaces_the_previous_store() {
    for corruption in ["mixed", "invalid", "unknown"] {
        let (_directory, store, expected) = previous_store();
        let path = provider_path(&store, &expected.providers[&AppKind::Codex][0].id);
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        match corruption {
            "mixed" => {
                value["responsesOptions"]
                    .as_object_mut()
                    .unwrap()
                    .remove("supportsWebsockets");
            }
            "invalid" => value["responsesOptions"]["supportsWebsockets"] = json!("false"),
            _ => fs::write(store.configuration_dir().join("personal.txt"), "retain").unwrap(),
        }
        fs::write(&path, serde_json::to_vec_pretty(&value).unwrap()).unwrap();
        let before = layout::fingerprint(&store.configuration_dir()).unwrap();
        assert!(store.upgrade_if_needed().is_err(), "{corruption}");
        assert_eq!(
            layout::fingerprint(&store.configuration_dir()).unwrap(),
            before
        );
        assert!(!journal_path(&store).exists());
        assert!(backups(&store).is_empty());
    }
}

#[test]
fn interrupted_responses_activation_keeps_the_preimage_backup() {
    let (_directory, store, expected) = previous_store();
    let before = layout::fingerprint(&store.configuration_dir()).unwrap();
    let journal = recovery::UpgradeJournal::new();
    let (staging, retired) = journal.paths(&store).unwrap();
    stage_and_verify(&staging.join("configuration"), &expected).unwrap();
    write_json_atomic(
        &journal_path(&store),
        &serde_json::to_string(&journal).unwrap(),
    )
    .unwrap();
    fs::rename(store.configuration_dir(), &retired).unwrap();
    fs::rename(staging.join("configuration"), store.configuration_dir()).unwrap();
    assert!(!store.upgrade_if_needed().unwrap());
    assert_eq!(read_configuration_snapshot(&store).unwrap(), expected);
    assert_eq!(layout::fingerprint(&retired).unwrap(), before);
    assert!(!journal_path(&store).exists());
}
