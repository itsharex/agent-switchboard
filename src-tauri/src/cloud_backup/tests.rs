use super::crypto::{decrypt, encrypt};
use super::*;
use crate::config_store::snapshot::ConfigurationSnapshot;

#[test]
fn backup_rejects_an_incorrect_password_without_decoding_the_snapshot() {
    let snapshot = ConfigurationSnapshot {
        schema_version: crate::config_store::snapshot::CLOUD_BACKUP_SNAPSHOT_SCHEMA_VERSION,
        codex_providers: vec![],
        codex_official: vec![],
        claude_providers: vec![],
        client_settings: Default::default(),
        history: Default::default(),
    };
    let encrypted = encrypt(&snapshot, "cloud-backup-password").unwrap();
    assert_eq!(
        decrypt(&encrypted, "wrong-password").unwrap_err(),
        "备份密码不正确或云端备份已损坏"
    );
}

#[test]
fn backup_table_sql_restricts_rows_to_the_authenticated_user() {
    assert!(SETUP_SQL.contains("enable row level security"));
    assert!(SETUP_SQL.contains("to authenticated"));
    assert!(SETUP_SQL.contains("(select auth.uid()) = user_id"));
}

#[test]
fn upload_record_uses_database_column_names() {
    let payload = EncryptedBackup {
        version: ENCRYPTION_VERSION,
        salt: "salt".to_string(),
        nonce: "nonce".to_string(),
        ciphertext: "ciphertext".to_string(),
    };
    let record = serde_json::to_value(UploadRecord {
        user_id: "user-id",
        payload: &payload,
        updated_at: "2026-09-09T00:00:00Z",
    })
    .unwrap();
    assert_eq!(record["user_id"], "user-id");
    assert!(record.get("userId").is_none());
    assert!(record.get("updatedAt").is_none());
}
