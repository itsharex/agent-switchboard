use super::crypto::{decrypt, encrypt};
use super::{
    configured_settings, validate_account_password, validate_passwords, AuthResponse,
    AuthenticatedUser, CloudBackupResult, EncryptedBackup, RemoteRecord, UploadRecord, TABLE,
};
use crate::config_store::snapshot::{
    decode_cloud_backup_snapshot, enable_snapshot, read_configuration_snapshot,
};
use crate::local_state::{CloudBackupSettings, LocalState};
use serde::de::DeserializeOwned;
use uuid::Uuid;

pub(super) fn upload_with_request<F>(
    state: &LocalState,
    account_password: &str,
    backup_password: &str,
    request: &F,
) -> Result<CloudBackupResult, String>
where
    F: Fn(&str, &str, &str, &[u8]) -> Result<(u16, String), String>,
{
    let settings = configured_settings(state)?;
    validate_passwords(account_password, backup_password)?;
    let user = authenticate_with_request(&settings, account_password, request)?;
    let snapshot =
        read_configuration_snapshot(&state.configuration()).map_err(|error| error.to_string())?;
    let payload = encrypt(&snapshot, backup_password)?;
    let updated_at = backup_updated_at();
    upload_payload_with_request(&settings, &user, &payload, &updated_at, request)?;
    Ok(CloudBackupResult {
        updated_at,
        profile_count: snapshot.provider_count(),
    })
}

pub(super) fn restore_with_request<F>(
    state: &LocalState,
    account_password: &str,
    backup_password: &str,
    request: &F,
) -> Result<CloudBackupResult, String>
where
    F: Fn(&str, &str, &str, &[u8]) -> Result<(u16, String), String>,
{
    let settings = configured_settings(state)?;
    validate_passwords(account_password, backup_password)?;
    let user = authenticate_with_request(&settings, account_password, request)?;
    let headers = data_headers(&settings, &user.access_token, "return=representation");
    let url = format!(
        "{}/rest/v1/{}?select=payload,updated_at&user_id=eq.{}&limit=1",
        settings.project_url, TABLE, user.id
    );
    let (status, body) = request("GET", &url, &headers, &[])?;
    if status != 200 {
        return Err(remote_table_error(status));
    }
    let records: Vec<RemoteRecord> = decode_json(&body, "云端备份响应无效")?;
    let record = records
        .into_iter()
        .next()
        .ok_or_else(|| "云端没有可恢复的备份".to_string())?;
    let mut cleartext = decrypt(&record.payload, backup_password)?;
    let decoded = decode_cloud_backup_snapshot(&cleartext);
    cleartext.fill(0);
    let snapshot = decoded?;
    enable_snapshot(&state.configuration(), &snapshot)?;
    Ok(CloudBackupResult {
        updated_at: record.updated_at,
        profile_count: snapshot.provider_count(),
    })
}

fn upload_payload_with_request<F>(
    settings: &CloudBackupSettings,
    user: &AuthenticatedUser,
    payload: &EncryptedBackup,
    updated_at: &str,
    request: &F,
) -> Result<(), String>
where
    F: Fn(&str, &str, &str, &[u8]) -> Result<(u16, String), String>,
{
    let body = serde_json::to_vec(&UploadRecord {
        user_id: &user.id,
        payload,
        updated_at,
    })
    .map_err(|_| "云端备份序列化失败".to_string())?;
    let headers = data_headers(
        settings,
        &user.access_token,
        "resolution=merge-duplicates,return=minimal",
    );
    let url = format!(
        "{}/rest/v1/{}?on_conflict=user_id",
        settings.project_url, TABLE
    );
    let (status, _) = request("POST", &url, &headers, &body)?;
    if status != 200 && status != 201 {
        return Err(remote_table_error(status));
    }
    Ok(())
}

fn backup_updated_at() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(super) fn test_connection_with_request<F>(
    settings: &CloudBackupSettings,
    account_password: &str,
    request: F,
) -> Result<(), String>
where
    F: Fn(&str, &str, &str, &[u8]) -> Result<(u16, String), String>,
{
    settings.validate()?;
    validate_account_password(account_password)?;
    let user = authenticate_with_request(settings, account_password, &request)?;
    verify_remote_backup_table_with_request(settings, &user, &request)
}

fn authenticate_with_request<F>(
    settings: &CloudBackupSettings,
    account_password: &str,
    request: &F,
) -> Result<AuthenticatedUser, String>
where
    F: Fn(&str, &str, &str, &[u8]) -> Result<(u16, String), String>,
{
    let body = serde_json::to_vec(&serde_json::json!({
        "email": settings.email,
        "password": account_password,
    }))
    .map_err(|_| "项目 Auth 登录请求序列化失败".to_string())?;
    let headers = format!(
        "apikey: {}\r\nContent-Type: application/json",
        settings.publishable_key
    );
    let url = format!("{}/auth/v1/token?grant_type=password", settings.project_url);
    let (status, body) = request("POST", &url, &headers, &body)?;
    if status != 200 {
        return Err("项目 Auth 登录失败，请检查邮箱、密码和项目设置".to_string());
    }
    let response: AuthResponse = decode_json(&body, "项目 Auth 登录响应无效")?;
    let id = Uuid::parse_str(&response.user.id)
        .map_err(|_| "项目 Auth 登录响应无效".to_string())?
        .to_string();
    if response.access_token.is_empty() {
        return Err("项目 Auth 登录响应无效".to_string());
    }
    Ok(AuthenticatedUser {
        id,
        access_token: response.access_token,
    })
}

fn verify_remote_backup_table_with_request<F>(
    settings: &CloudBackupSettings,
    user: &AuthenticatedUser,
    request: &F,
) -> Result<(), String>
where
    F: Fn(&str, &str, &str, &[u8]) -> Result<(u16, String), String>,
{
    let headers = data_headers(settings, &user.access_token, "return=minimal");
    let url = format!(
        "{}/rest/v1/{}?select=user_id&user_id=eq.{}&limit=1",
        settings.project_url, TABLE, user.id
    );
    let (status, _) = request("GET", &url, &headers, &[])?;
    if status != 200 {
        return Err(remote_table_error(status));
    }
    Ok(())
}

fn data_headers(settings: &CloudBackupSettings, access_token: &str, prefer: &str) -> String {
    format!(
        "apikey: {}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nPrefer: {prefer}",
        settings.publishable_key, access_token
    )
}

fn remote_table_error(status: u16) -> String {
    if status == 404 {
        "云端备份表不可用，请确认已启用 Data API 并在 Supabase SQL Editor 执行初始化 SQL"
            .to_string()
    } else if status == 401 || status == 403 {
        "云端备份表权限不足，请在 Supabase SQL Editor 重新执行初始化 SQL".to_string()
    } else {
        format!("Supabase 云端备份请求失败（HTTP {status}）")
    }
}

fn decode_json<T: DeserializeOwned>(body: &str, message: &str) -> Result<T, String> {
    serde_json::from_str(body).map_err(|_| message.to_string())
}
