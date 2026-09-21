//! Auth side of a confirmed Codex configuration restore, including crash recovery identity.
use crate::{commands::error::CommandError, local_state::LocalState};
use asb_core::{AppKind, BackupRecord};
use asb_switch::{FsIo, SwitchIo};
use std::path::Path;
pub(super) fn intent(
    state: &LocalState,
    record: &BackupRecord,
    target: &Path,
    candidate_config: &str,
) -> Result<Option<super::transaction::AuthIntent>, CommandError> {
    if record.app != AppKind::Codex {
        return Ok(None);
    }
    let path = target.with_file_name("auth.json");
    let records = asb_switch::list_backups(&FsIo, &state.backup_dir())
        .into_iter()
        .filter(|backup| backup.linked_backup_id.as_deref() == Some(record.id.as_str()))
        .collect::<Vec<_>>();
    let source = match records.as_slice() {
        [] => {
            validate_current_auth(target, candidate_config)?;
            return Ok(None);
        }
        [source] => source,
        _ => return Err(error("Codex 配置备份关联多个认证来源，已拒绝恢复")),
    };
    if Path::new(&source.target_path) != path || source.reason != "codex-auth-projection" {
        return Err(error("Codex 认证备份目标或版本不匹配"));
    }
    let after = FsIo
        .read_file(Path::new(&source.backup_path))
        .map_err(|_| error("Codex 认证备份不可读"))?;
    if asb_switch::sha256_hex(&after) != source.content_hash {
        return Err(error("Codex 认证备份哈希不匹配"));
    }
    validate_direct_auth(
        candidate_config,
        source.target_existed.then_some(after.as_str()),
    )?;
    crate::codex_auth::projection::validate_backup(state.root(), &after)
        .map_err(|message| error(&message))?;
    let (before, existed) = match FsIo.read_file(&path) {
        Ok(content) => (content, true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (String::new(), false),
        Err(_) => return Err(error("Codex 当前认证文件不可读")),
    };
    Ok(Some(super::transaction::AuthIntent {
        before_hash: asb_switch::sha256_hex(&before),
        before_existed: existed,
        after_hash: source.content_hash.clone(),
        after_existed: source.target_existed,
    }))
}
fn error(message: &str) -> CommandError {
    CommandError::new("codex-auth-restore-invalid", message)
}

pub(super) fn validate_current_auth(target: &Path, config: &str) -> Result<(), CommandError> {
    let current = match FsIo.read_file(&target.with_file_name("auth.json")) {
        Ok(text) => Some(text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err(error("Codex 当前认证文件不可读")),
    };
    validate_direct_auth(config, current.as_deref())
}

fn validate_direct_auth(config: &str, auth: Option<&str>) -> Result<(), CommandError> {
    let document = config
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| error("Codex 备份配置格式无效"))?;
    let endpoint = document
        .get("openai_base_url")
        .and_then(|value| value.as_str());
    if endpoint.is_none_or(asb_core::adapter::codex::is_gateway_base_url) {
        return Ok(());
    }
    let value = auth.and_then(|text| serde_json::from_str::<serde_json::Value>(text).ok());
    let usable = value.as_ref().is_some_and(|value| {
        value
            .get("OPENAI_API_KEY")
            .and_then(|v| v.as_str())
            .is_some_and(|key| !key.trim().is_empty())
            && value
                .get("auth_mode")
                .and_then(|v| v.as_str())
                .is_none_or(|mode| mode == "apikey")
    });
    if !usable {
        return Err(error(
            "此直连备份没有配套的 API-key 认证，恢复可能误用官方登录；请重新应用目标 Codex 供应商",
        ));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_restore_cannot_route_official_oauth_to_a_third_party() {
        let config = "model_provider = 'openai'\nopenai_base_url = 'https://third.example/v1'";
        for auth in [
            None,
            Some(r#"{"auth_mode":"chatgpt","tokens":{"access_token":"never-forward"}}"#),
            Some(r#"{"auth_mode":"chatgpt","OPENAI_API_KEY":"key"}"#),
        ] {
            assert!(validate_direct_auth(config, auth).is_err());
        }
        assert!(validate_direct_auth(
            config,
            Some(r#"{"auth_mode":"apikey","OPENAI_API_KEY":"key"}"#)
        )
        .is_ok());
        assert!(validate_direct_auth("model_provider = 'openai'", None).is_ok());
    }
}
