//! One versioned application-owned file, independent of both clients' login caches.

use super::contracts::AccountFile;
use asb_switch::sha256_hex;
use std::path::{Path, PathBuf};

pub(crate) fn path(root: &Path) -> PathBuf {
    root.join("claude-accounts.json")
}

pub(super) fn load(root: &Path) -> Result<(AccountFile, String), String> {
    let text = crate::config_store::read_optional(&path(root))
        .map_err(|_| "Claude 托管账号文件不可读；原文件未修改".to_string())?;
    let hash = sha256_hex(text.as_deref().unwrap_or(""));
    let file: AccountFile = match text {
        Some(text) => crate::config_store::parse_strict(&text)
            .map_err(|_| "Claude 托管账号文件格式无效；请修复文件，原内容未修改".to_string())?,
        None => AccountFile::default(),
    };
    file.validate()?;
    Ok((file, hash))
}

pub(super) fn save(root: &Path, file: &AccountFile, expected_hash: &str) -> Result<String, String> {
    file.validate()?;
    let (_, current) = load(root)?;
    if current != expected_hash {
        return Err("Claude 托管账号已改变，请重新读取后再操作".into());
    }
    let text =
        serde_json::to_string_pretty(file).map_err(|_| "Claude 托管账号无法序列化".to_string())?;
    crate::config_store::write_json_atomic(&path(root), &text)
        .map_err(|error| format!("无法保存 Claude 托管账号：{error}"))?;
    Ok(sha256_hex(&text))
}
