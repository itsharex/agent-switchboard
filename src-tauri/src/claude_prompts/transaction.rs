//! Recoverable Claude prompt activation. Native writes are delegated to asb-switch under its lock.

use super::{contracts::*, list, preview, store};
use asb_core::AppKind;
use asb_switch::{
    read_global_prompt_document, sha256_hex, write_global_prompt_document_with_commit, FsIo,
    GlobalPromptDocumentRequest,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    version: u32,
    target: String,
    before_file_hash: String,
    before_file_text: String,
    after_file: PromptFile,
    before_live_hash: String,
    after_live_hash: String,
}

fn path(root: &Path) -> PathBuf {
    root.join("configuration/claude-prompts.pending.json")
}

pub(super) fn pending(root: &Path) -> bool {
    match std::fs::symlink_metadata(path(root)) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        _ => true,
    }
}

pub(super) fn require_clear(root: &Path) -> Result<(), String> {
    if pending(root) {
        Err("Claude 提示词存在待恢复事务，请先执行恢复；原文件未修改".into())
    } else {
        Ok(())
    }
}

pub(crate) fn activate(
    root: &Path,
    target: &Path,
    backups: &Path,
    plan: ClaudePromptActivation,
) -> Result<ClaudePromptsView, String> {
    let current = preview(root, target, plan.prompt_id.clone(), &plan.file_hash)?;
    if current.plan != plan {
        return Err("Claude 提示词或预览已变化，请重新预览后再确认".into());
    }
    let (before_file, file_hash, before_file_text) = store::snapshot(root)?;
    if file_hash != plan.file_hash {
        return Err("Claude 提示词库在准备事务时已改变，请重新预览".into());
    }
    let mut after_file = before_file.clone();
    after_file.active = plan.prompt_id.as_ref().map(|id| ActivePrompt {
        id: id.clone(),
        content_hash: plan.rendered_hash.clone(),
    });
    let journal = Journal {
        version: 1,
        target: target.to_string_lossy().into(),
        before_file_hash: plan.file_hash.clone(),
        before_file_text,
        after_file,
        before_live_hash: plan.live_hash.clone(),
        after_live_hash: plan.rendered_hash.clone(),
    };
    save_journal(root, &journal)?;
    write_global_prompt_document_with_commit(
        &FsIo,
        &GlobalPromptDocumentRequest {
            target,
            app: AppKind::Claude,
            content: &current.after,
            backup_dir: backups,
            expected_hash: &plan.live_hash,
        },
        |_| store::save(root, &journal.after_file, &plan.file_hash).map(|_| ()),
    )
    .map_err(|error| format!("Claude 提示词激活未完成：{error}；请执行提示词事务恢复"))?;
    clear(root)?;
    list(root, target)
}

fn save_journal(root: &Path, journal: &Journal) -> Result<(), String> {
    require_clear(root)?;
    validate_before(journal)?;
    journal.after_file.validate()?;
    let text = serde_json::to_string_pretty(journal).map_err(|_| "Claude 提示词事务无法编码")?;
    crate::config_store::write_json_atomic(&path(root), &text)
        .map_err(|error| format!("Claude 提示词事务无法保存：{error}"))
}

fn read_journal(root: &Path, target: &Path) -> Result<Journal, String> {
    if std::fs::metadata(path(root)).is_ok_and(|metadata| metadata.len() > 17 * 1024 * 1024) {
        return Err("Claude 提示词事务记录超过大小限制，原文件未修改".into());
    }
    let text = crate::config_store::read_optional(&path(root))
        .map_err(|error| format!("Claude 提示词事务不可读：{error}"))?
        .ok_or("Claude 提示词没有待恢复事务")?;
    let journal: Journal = crate::config_store::parse_strict(&text)
        .map_err(|_| "Claude 提示词事务记录无效，原文件未修改")?;
    if journal.version != 1 || journal.target != target.to_string_lossy() {
        return Err("Claude 提示词事务版本或目标路径不匹配，未改写任何文件".into());
    }
    validate_before(&journal)?;
    journal.after_file.validate()?;
    if sha256_hex(&journal.before_file_text) != journal.before_file_hash
        || journal
            .after_file
            .active
            .as_ref()
            .is_some_and(|active| active.content_hash != journal.after_live_hash)
    {
        return Err("Claude 提示词事务修订不匹配，未改写任何文件".into());
    }
    let expected_after = journal
        .after_file
        .active
        .as_ref()
        .and_then(|active| {
            journal
                .after_file
                .prompts
                .iter()
                .find(|p| p.id == active.id)
        })
        .map(|p| p.draft.content.as_str())
        .unwrap_or("");
    if sha256_hex(expected_after) != journal.after_live_hash {
        return Err("Claude 提示词事务内容与目标摘要不匹配".into());
    }
    Ok(journal)
}

pub(crate) fn recover(root: &Path, target: &Path) -> Result<ClaudePromptsView, String> {
    if !pending(root) {
        return list(root, target);
    }
    let journal = read_journal(root, target)?;
    let (_, current_hash) = store::load(root)?;
    let after_hash = sha256_hex(&store::encode(&journal.after_file)?);
    if current_hash != journal.before_file_hash && current_hash != after_hash {
        return Err("Claude 提示词库在事务期间已外改；保留现场，请核对后恢复".into());
    }
    let live = read_global_prompt_document(&FsIo, target, AppKind::Claude)
        .map_err(|error| error.to_string())?;
    if live.content_hash == journal.after_live_hash {
        if current_hash != after_hash {
            store::save(root, &journal.after_file, &current_hash)?;
        }
    } else if live.content_hash == journal.before_live_hash {
        if current_hash != journal.before_file_hash {
            restore_before(root, &journal, &current_hash)?;
        }
    } else {
        return Err("CLAUDE.md 在事务期间已外改；保留文件与事务记录，不覆盖外部内容".into());
    }
    clear(root)?;
    list(root, target)
}

fn clear(root: &Path) -> Result<(), String> {
    std::fs::remove_file(path(root))
        .map_err(|error| format!("Claude 提示词状态已提交，但事务记录未清理：{error}；可重试恢复"))
}

#[cfg(test)]
pub(super) fn interrupt_after_document(
    root: &Path,
    target: &Path,
    plan: &ClaudePromptActivation,
    content: &str,
) {
    let (before_file, _) = store::load(root).unwrap();
    let mut after_file = before_file.clone();
    after_file.active = plan.prompt_id.as_ref().map(|id| ActivePrompt {
        id: id.clone(),
        content_hash: plan.rendered_hash.clone(),
    });
    save_journal(
        root,
        &Journal {
            version: 1,
            target: target.to_string_lossy().into(),
            before_file_hash: plan.file_hash.clone(),
            before_file_text: crate::config_store::read_optional(&store::path(root))
                .unwrap()
                .unwrap(),
            after_file,
            before_live_hash: plan.live_hash.clone(),
            after_live_hash: plan.rendered_hash.clone(),
        },
    )
    .unwrap();
    std::fs::write(target, content).unwrap();
}

fn validate_before(journal: &Journal) -> Result<(), String> {
    if sha256_hex(&journal.before_file_text) != journal.before_file_hash {
        return Err("Claude 提示词事务原状态摘要不匹配".into());
    }
    let file: PromptFile = crate::config_store::parse_strict(&journal.before_file_text)
        .map_err(|_| "Claude 提示词事务原状态无效")?;
    file.validate()
}

fn restore_before(root: &Path, journal: &Journal, expected_hash: &str) -> Result<(), String> {
    if store::load(root)?.1 != expected_hash {
        return Err("Claude 提示词库在恢复期间已改变".into());
    }
    crate::config_store::write_json_atomic(&store::path(root), &journal.before_file_text)
        .map_err(|error| error.to_string())
}
