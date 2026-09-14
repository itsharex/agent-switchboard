use super::{contracts::PromptsFile, store};
use asb_switch::{
    read_global_prompt_document, write_global_prompt_document_with_commit, FsIo,
    GlobalPromptDocumentRequest,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Intent {
    version: u8,
    before: PromptsFile,
    after: PromptsFile,
    before_live_hash: String,
    before_live_existed: bool,
    after_live_hash: String,
}
pub(super) fn pending(root: &Path) -> std::path::PathBuf {
    root.join("codex/prompt-switch.json")
}
pub(super) fn apply(
    root: &Path,
    target: &Path,
    backups: &Path,
    after: PromptsFile,
    content: &str,
    expected_revision: &str,
    expected_live: &str,
) -> Result<(), String> {
    store::validate(&after)?;
    if pending(root).exists() {
        return Err("Codex 指令预设有待恢复事务，请先恢复".into());
    }
    let (before, revision) = store::load(root)?;
    if revision != expected_revision {
        return Err("Codex 指令预设库已变化，请重新预览".into());
    }
    let live = read_global_prompt_document(&FsIo, target, asb_core::AppKind::Codex)
        .map_err(|e| e.to_string())?;
    if live.content_hash != expected_live {
        return Err("Codex 全局指令已在预览后外改，请重新读取".into());
    }
    let intent = Intent {
        version: 1,
        before,
        after: after.clone(),
        before_live_hash: live.content_hash.clone(),
        before_live_existed: live.exists,
        after_live_hash: asb_switch::sha256_hex(content),
    };
    let encoded = serde_json::to_string_pretty(&intent).map_err(|_| "Codex 指令事务无法序列化")?;
    crate::config_store::write_json_atomic(&pending(root), &encoded)?;
    let result = write_global_prompt_document_with_commit(
        &FsIo,
        &GlobalPromptDocumentRequest {
            target,
            app: asb_core::AppKind::Codex,
            content,
            backup_dir: backups,
            expected_hash: expected_live,
        },
        |_| store::save(root, &after, expected_revision).map(|_| ()),
    );
    match result {
        Ok(_) => clear(root),
        Err(error) => match recover(root, target) {
            Ok(()) => Err(error.to_string()),
            Err(recovery) => Err(format!("{error}；{recovery}")),
        },
    }
}
pub(super) fn recover(root: &Path, target: &Path) -> Result<(), String> {
    let raw = match fs::read_to_string(pending(root)) {
        Ok(raw) => raw,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err("Codex 指令事务日志不可读".into()),
    };
    let intent: Intent =
        serde_json::from_str(&raw).map_err(|_| "Codex 指令事务日志无效，已保留现场")?;
    if intent.version != 1 {
        return Err("Codex 指令事务版本不受支持".into());
    }
    let (file, revision) = store::load(root)?;
    if file != intent.before && file != intent.after {
        return Err("Codex 指令库已在事务期间外改，未覆盖".into());
    }
    let live = read_global_prompt_document(&FsIo, target, asb_core::AppKind::Codex)
        .map_err(|e| e.to_string())?;
    let next = if live.content_hash == intent.after_live_hash && live.exists {
        &intent.after
    } else if live.content_hash == intent.before_live_hash
        && live.exists == intent.before_live_existed
    {
        &intent.before
    } else {
        return Err("Codex 全局指令已在事务期间外改，待恢复日志与备份已保留".into());
    };
    if &file != next {
        store::save(root, next, &revision)?;
    }
    clear(root)
}
fn clear(root: &Path) -> Result<(), String> {
    fs::remove_file(pending(root))
        .map_err(|_| "Codex 指令已提交，但事务标记清理失败，请重试恢复".into())
}
