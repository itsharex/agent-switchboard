//! Encrypted, user-owned Supabase backups for the application profile store.
//!
//! This module owns the only remote-backup contract: Supabase authenticates
//! the user and enforces row ownership, while an independent user password
//! derives the AES-GCM key that protects the complete profile store before it
//! leaves this device. No Supabase secret/service key, sign-in password, or
//! access token is persisted locally.

mod crypto;
mod remote;


use crate::local_state::{CloudBackupSettings, LocalState};
use crate::probe::http_request;
use remote::{restore_with_request, test_connection_with_request, upload_with_request};
use serde::{Deserialize, Serialize};

const TABLE: &str = "agent_switchboard_cloud_backups";
const ENCRYPTION_VERSION: u8 = 1;
const SALT_LENGTH: usize = 16;
const NONCE_LENGTH: usize = 12;
const KEY_LENGTH: usize = 32;
const AAD: &[u8] = b"agent-switchboard-cloud-backup-v1";

/// SQL the user runs once in their own Supabase SQL Editor. It deliberately
/// grants only the operations this client needs and leaves no delete path.
pub const SETUP_SQL: &str = r#"create table if not exists public.agent_switchboard_cloud_backups (
  user_id uuid primary key references auth.users(id) on delete cascade,
  payload jsonb not null,
  updated_at timestamptz not null default now()
);

alter table public.agent_switchboard_cloud_backups enable row level security;
revoke all on table public.agent_switchboard_cloud_backups from anon;
grant select, insert, update on table public.agent_switchboard_cloud_backups to authenticated;

drop policy if exists "read own encrypted backup" on public.agent_switchboard_cloud_backups;
create policy "read own encrypted backup"
on public.agent_switchboard_cloud_backups
for select to authenticated
using ((select auth.uid()) = user_id);

drop policy if exists "insert own encrypted backup" on public.agent_switchboard_cloud_backups;
create policy "insert own encrypted backup"
on public.agent_switchboard_cloud_backups
for insert to authenticated
with check ((select auth.uid()) = user_id);

drop policy if exists "update own encrypted backup" on public.agent_switchboard_cloud_backups;
create policy "update own encrypted backup"
on public.agent_switchboard_cloud_backups
for update to authenticated
using ((select auth.uid()) = user_id)
with check ((select auth.uid()) = user_id);"#;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudBackupResult {
    pub updated_at: String,
    pub profile_count: usize,
    pub migrated: bool,
}

#[derive(Debug, Serialize)]
struct UploadRecord<'a> {
    user_id: &'a str,
    payload: &'a EncryptedBackup,
    updated_at: &'a str,
}

#[derive(Debug, Deserialize)]
struct AuthResponse {
    #[serde(rename = "access_token")]
    access_token: String,
    user: AuthUser,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthUser {
    id: String,
}

#[derive(Debug, Deserialize)]
struct RemoteRecord {
    payload: EncryptedBackup,
    updated_at: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct EncryptedBackup {
    version: u8,
    salt: String,
    nonce: String,
    ciphertext: String,
}

struct AuthenticatedUser {
    id: String,
    access_token: String,
}

/// Returns the persisted public connection coordinates, if the user has
/// configured their own Supabase project.
pub fn settings(state: &LocalState) -> Result<Option<CloudBackupSettings>, String> {
    state.get_cloud_backup_settings()
}

/// Stores connection coordinates only. Passwords are command arguments and
/// intentionally never enter this local state file.
pub fn save_settings(state: &LocalState, settings: CloudBackupSettings) -> Result<(), String> {
    state.set_cloud_backup_settings(&settings)
}

/// Validates the current, unsaved connection draft without creating or
/// replacing a remote backup. A successful result proves that the configured
/// Auth user can read only its own cloud-backup row.
pub fn test_connection(
    settings: &CloudBackupSettings,
    account_password: &str,
) -> Result<(), String> {
    test_connection_with_request(settings, account_password, http_request)
}

pub fn upload(
    state: &LocalState,
    account_password: &str,
    backup_password: &str,
) -> Result<CloudBackupResult, String> {
    upload_with_request(state, account_password, backup_password, &http_request)
}
pub fn restore(
    state: &LocalState,
    account_password: &str,
    backup_password: &str,
) -> Result<CloudBackupResult, String> {
    restore_with_request(state, account_password, backup_password, &http_request)
}

fn configured_settings(state: &LocalState) -> Result<CloudBackupSettings, String> {
    state
        .get_cloud_backup_settings()?
        .ok_or_else(|| "请先保存 Supabase 云端备份设置".to_string())
}

fn validate_passwords(account_password: &str, backup_password: &str) -> Result<(), String> {
    validate_account_password(account_password)?;
    if backup_password.len() < 8 {
        return Err("备份密码至少需要 8 个字符".to_string());
    }
    Ok(())
}

fn validate_account_password(account_password: &str) -> Result<(), String> {
    if account_password.is_empty() {
        return Err("请输入项目 Auth 登录密码".to_string());
    }
    Ok(())
}
