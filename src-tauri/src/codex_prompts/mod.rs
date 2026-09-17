//! Named Codex prompts, projected only to the user-global AGENTS.md.
//! Inactive edits never touch live instructions or Claude's prompt library.
pub(crate) mod contracts;
mod store;
#[cfg(test)]
mod tests;
mod transaction;
use asb_switch::{read_global_prompt_document, FsIo};
use contracts::*;
use std::{path::Path, sync::Mutex};
static LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn list(root: &Path, target: &Path) -> Result<PromptsView, String> {
    let _guard = LOCK.lock().map_err(|_| "Codex 指令库锁不可用")?;
    view(root, target)
}
fn view(root: &Path, target: &Path) -> Result<PromptsView, String> {
    let (file, revision) = store::load(root)?;
    let live = read_global_prompt_document(&FsIo, target, asb_core::AppKind::Codex)
        .map_err(|e| e.to_string())?;
    Ok(PromptsView {
        revision,
        active_id: file.active_id,
        presets: file.presets,
        live,
        pending_recovery: transaction::pending(root).exists()
            || matches!(
                asb_switch::lockfile::probe_lock(&FsIo, target),
                asb_core::LockStatus::Stale { .. } | asb_core::LockStatus::Indeterminate { .. }
            ),
    })
}
fn require_revision(root: &Path, expected: &str) -> Result<PromptsFile, String> {
    if transaction::pending(root).exists() {
        return Err("Codex 指令有待恢复事务，请先恢复".into());
    }
    let (file, revision) = store::load(root)?;
    if revision != expected {
        return Err("Codex 指令库已变化，请重新读取".into());
    }
    Ok(file)
}
pub(crate) fn save(
    root: &Path,
    target: &Path,
    backups: &Path,
    id: Option<&str>,
    draft: PromptDraft,
    revision: &str,
    live_hash: Option<&str>,
    confirm: bool,
) -> Result<PromptsView, String> {
    let _guard = LOCK.lock().map_err(|_| "Codex 指令库锁不可用")?;
    store::validate_draft(&draft)?;
    let mut file = require_revision(root, revision)?;
    let now = chrono::Utc::now().to_rfc3339();
    let content = draft.content.clone();
    if let Some(id) = id {
        let preset = file
            .presets
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or("Codex 指令预设不存在")?;
        preset.draft = draft;
        preset.updated_at = now;
    } else {
        file.presets.push(PromptPreset {
            id: uuid::Uuid::new_v4().to_string(),
            draft,
            created_at: now.clone(),
            updated_at: now,
        });
    }
    if id.is_some() && file.active_id.as_deref() == id {
        if !confirm {
            return Err("保存正在使用的 Codex 指令预设需要确认写入 AGENTS.md".into());
        }
        transaction::apply(
            root,
            target,
            backups,
            file,
            &content,
            revision,
            live_hash.ok_or("请先读取 Codex 全局指令")?,
        )?;
    } else {
        store::save(root, &file, revision)?;
    }
    view(root, target)
}
pub(crate) fn delete(
    root: &Path,
    target: &Path,
    id: &str,
    revision: &str,
) -> Result<PromptsView, String> {
    let _guard = LOCK.lock().map_err(|_| "Codex 指令库锁不可用")?;
    let mut file = require_revision(root, revision)?;
    if file.active_id.as_deref() == Some(id) {
        return Err("请先停用 Codex 指令预设，再删除".into());
    }
    let count = file.presets.len();
    file.presets.retain(|p| p.id != id);
    if count == file.presets.len() {
        return Err("Codex 指令预设不存在".into());
    }
    store::save(root, &file, revision)?;
    view(root, target)
}
pub(crate) fn preview(
    root: &Path,
    target: &Path,
    id: Option<String>,
    revision: &str,
) -> Result<PromptPreview, String> {
    let _guard = LOCK.lock().map_err(|_| "Codex 指令库锁不可用")?;
    let file = require_revision(root, revision)?;
    let live = read_global_prompt_document(&FsIo, target, asb_core::AppKind::Codex)
        .map_err(|e| e.to_string())?;
    let after = selected_content(&file, id.as_deref(), &live.content)?;
    Ok(PromptPreview {
        plan: PromptActivation {
            preset_id: id,
            revision: revision.into(),
            live_hash: live.content_hash,
            rendered_hash: asb_switch::sha256_hex(&after),
        },
        before: live.content,
        after,
        backfills_active: file.active_id.is_some(),
    })
}
fn selected_content(file: &PromptsFile, id: Option<&str>, live: &str) -> Result<String, String> {
    let Some(id) = id else {
        return Ok(String::new());
    };
    if file.active_id.as_deref() == Some(id) {
        return Ok(live.to_string());
    }
    file.presets
        .iter()
        .find(|p| p.id == id)
        .map(|p| p.draft.content.clone())
        .ok_or_else(|| "Codex 指令预设不存在".into())
}
pub(crate) fn activate(
    root: &Path,
    target: &Path,
    backups: &Path,
    plan: PromptActivation,
) -> Result<PromptsView, String> {
    let _guard = LOCK.lock().map_err(|_| "Codex 指令库锁不可用")?;
    let mut file = require_revision(root, &plan.revision)?;
    let live = read_global_prompt_document(&FsIo, target, asb_core::AppKind::Codex)
        .map_err(|e| e.to_string())?;
    if live.content_hash != plan.live_hash {
        return Err("Codex 全局指令在预览后已变化".into());
    }
    let content = selected_content(&file, plan.preset_id.as_deref(), &live.content)?;
    if asb_switch::sha256_hex(&content) != plan.rendered_hash {
        return Err("Codex 指令预览已失效".into());
    }
    backfill(&mut file, live.content);
    file.active_id = plan.preset_id;
    transaction::apply(
        root,
        target,
        backups,
        file,
        &content,
        &plan.revision,
        &plan.live_hash,
    )?;
    view(root, target)
}
fn backfill(file: &mut PromptsFile, content: String) {
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(preset) = file
        .presets
        .iter_mut()
        .find(|p| Some(&p.id) == file.active_id.as_ref())
    {
        preset.draft.content = content;
        preset.updated_at = now;
    } else if !content.is_empty() && !file.presets.iter().any(|p| p.draft.content == content) {
        file.presets.push(PromptPreset {
            id: uuid::Uuid::new_v4().to_string(),
            draft: PromptDraft {
                name: "原有 AGENTS.md".into(),
                description: Some("首次应用指令预设前保存的全局指令".into()),
                content,
            },
            created_at: now.clone(),
            updated_at: now,
        });
    }
}
pub(crate) fn recover(root: &Path, target: &Path) -> Result<PromptsView, String> {
    let _guard = LOCK.lock().map_err(|_| "Codex 指令库锁不可用")?;
    match asb_switch::lockfile::probe_lock(&FsIo, target) {
        asb_core::LockStatus::Free => {}
        asb_core::LockStatus::Stale { .. } => {
            asb_switch::lockfile::recover_stale(&FsIo, target)
                .map_err(|_| "Codex 指令锁已改变，未清理")?;
        }
        _ => return Err("Codex 指令正在写入或锁状态无法确认，请稍后重试；未删除锁".into()),
    }
    transaction::recover(root, target)?;
    view(root, target)
}

pub(crate) fn ensure_ready(root: &Path) -> Result<(), String> {
    if transaction::pending(root).exists() {
        return Err("Codex 指令预设有未完成事务，当前版本不再管理预设事务".into());
    }
    Ok(())
}
