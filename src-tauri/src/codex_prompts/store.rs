use super::contracts::{PromptDraft, PromptsFile};
use std::{collections::HashSet, fs, path::Path};
pub(super) fn path(root: &Path) -> std::path::PathBuf {
    root.join("codex/prompts.json")
}
pub(super) fn load(root: &Path) -> Result<(PromptsFile, String), String> {
    let raw = match fs::read_to_string(path(root)) {
        Ok(raw) => Some(raw),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err("无法读取 Codex 指令预设库".into()),
    };
    let file: PromptsFile = if raw.is_none() {
        PromptsFile::default()
    } else {
        serde_json::from_str(raw.as_deref().unwrap()).map_err(|_| "Codex 指令预设库格式无效")?
    };
    validate(&file)?;
    Ok((file, asb_switch::sha256_hex(raw.as_deref().unwrap_or(""))))
}
pub(super) fn validate_draft(draft: &PromptDraft) -> Result<(), String> {
    if draft.name.trim().is_empty()
        || draft.name.chars().count() > 120
        || draft.name.chars().any(char::is_control)
    {
        return Err("Codex 指令预设名称须为 1–120 个字符".into());
    }
    if draft.content.len() > 2 * 1024 * 1024
        || draft.content.contains('\0')
        || draft.description.as_ref().is_some_and(|s| s.len() > 4000)
    {
        return Err("Codex 指令内容或说明超过限制，或包含无效字符".into());
    }
    Ok(())
}
pub(super) fn validate(file: &PromptsFile) -> Result<(), String> {
    if file.version != 1 || file.presets.len() > 500 {
        return Err("Codex 指令预设版本或数量无效".into());
    }
    let mut ids = HashSet::new();
    for preset in &file.presets {
        validate_draft(&preset.draft)?;
        if uuid::Uuid::parse_str(&preset.id).is_err() || !ids.insert(&preset.id) {
            return Err("Codex 指令预设标识无效或重复".into());
        }
    }
    if file.active_id.as_ref().is_some_and(|id| !ids.contains(id)) {
        return Err("Codex 当前指令预设不存在".into());
    }
    Ok(())
}
pub(super) fn save(root: &Path, file: &PromptsFile, expected: &str) -> Result<String, String> {
    validate(file)?;
    if load(root)?.1 != expected {
        return Err("Codex 指令预设库已变化，请重新读取".into());
    }
    let content = serde_json::to_string_pretty(file).map_err(|_| "Codex 指令预设无法序列化")?;
    crate::config_store::write_json_atomic(&path(root), &content)?;
    Ok(asb_switch::sha256_hex(&content))
}
