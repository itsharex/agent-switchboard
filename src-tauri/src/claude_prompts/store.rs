//! One revision-guarded Claude library. Corrupt source bytes are never silently reset.

use super::contracts::PromptFile;
use asb_switch::sha256_hex;
use std::path::{Path, PathBuf};

pub(super) fn path(root: &Path) -> PathBuf {
    root.join("configuration/claude-prompts.json")
}

pub(super) fn load(root: &Path) -> Result<(PromptFile, String), String> {
    snapshot(root).map(|(file, hash, _)| (file, hash))
}

pub(super) fn snapshot(root: &Path) -> Result<(PromptFile, String, String), String> {
    let file = path(root);
    if std::fs::metadata(&file).is_ok_and(|metadata| metadata.len() > 8 * 1024 * 1024) {
        return Err("Claude 提示词库超过 8 MiB，未读取或重写".into());
    }
    let text = crate::config_store::read_optional(&file)
        .map_err(|error| format!("Claude 提示词库不可读：{error}"))?;
    let hash = sha256_hex(text.as_deref().unwrap_or(""));
    let library: PromptFile = match &text {
        Some(text) => crate::config_store::parse_strict(&text)
            .map_err(|_| "Claude 提示词库格式无效；请修复，原文件未修改")?,
        None => PromptFile::default(),
    };
    library.validate()?;
    Ok((library, hash, text.unwrap_or_default()))
}

pub(super) fn encode(library: &PromptFile) -> Result<String, String> {
    library.validate()?;
    let text = serde_json::to_string_pretty(library).map_err(|_| "Claude 提示词库无法编码")?;
    if text.len() > 8 * 1024 * 1024 {
        return Err("Claude 提示词库不能超过 8 MiB".into());
    }
    Ok(text)
}

pub(super) fn save(
    root: &Path,
    library: &PromptFile,
    expected_hash: &str,
) -> Result<String, String> {
    let text = encode(library)?;
    if load(root)?.1 != expected_hash {
        return Err("Claude 提示词库已改变，请重新读取后再操作".into());
    }
    crate::config_store::write_json_atomic(&path(root), &text)
        .map_err(|error| format!("无法保存 Claude 提示词库：{error}"))?;
    Ok(sha256_hex(&text))
}
