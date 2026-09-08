use super::*;

#[test]
fn previous_responses_snapshot_is_converted_once_without_changing_profile_facts() {
    let (_directory, _store, current) = live_snapshot();
    for enabled in [true, false] {
        let mut previous = serde_json::to_value(&current).unwrap();
        previous["schemaVersion"] = 4.into();
        previous["providers"]["codex"][0]["responsesOptions"]["supportsWebsockets"] =
            enabled.into();
        let converted =
            decode_cloud_backup_snapshot(&serde_json::to_vec(&previous).unwrap()).unwrap();
        assert!(converted.migrated);
        assert_eq!(converted.snapshot, current);
        let encoded = serde_json::to_vec(&converted.snapshot).unwrap();
        assert!(!decode_cloud_backup_snapshot(&encoded).unwrap().migrated);
        previous["schemaVersion"] = CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION.into();
        assert!(decode_cloud_backup_snapshot(&serde_json::to_vec(&previous).unwrap()).is_err());
        previous["schemaVersion"] = 4.into();
        previous["providers"]["codex"][0]["responsesOptions"]["supportsWebsockets"] =
            "invalid".into();
        assert!(decode_cloud_backup_snapshot(&serde_json::to_vec(&previous).unwrap()).is_err());
    }
}
