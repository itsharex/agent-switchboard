//! Claude prompt CRUD and previews. Only the transaction module may request a native write.

pub(crate) mod contracts;
pub(crate) mod source;
mod store;
#[cfg(test)]
mod tests;
mod transaction;
use asb_core::AppKind;
use asb_switch::{read_global_prompt_document, sha256_hex, FsIo};
use contracts::*;
use std::path::Path;
pub(crate) use transaction::{activate, recover};

pub(crate) fn list(root: &Path, target: &Path) -> Result<ClaudePromptsView, String> {
    let (library, file_hash) = store::load(root)?;
    let live = read_global_prompt_document(&FsIo, target, AppKind::Claude)
        .map_err(|error| error.to_string())?;
    let active = library.active.as_ref();
    Ok(ClaudePromptsView {
        file_hash,
        active_prompt_id: active.map(|active| active.id.clone()),
        external_change: active.is_some_and(|active| active.content_hash != live.content_hash),
        pending_content: active.is_some_and(|active| {
            library
                .prompts
                .iter()
                .find(|prompt| prompt.id == active.id)
                .is_some_and(|prompt| sha256_hex(&prompt.draft.content) != active.content_hash)
        }),
        live_hash: live.content_hash,
        prompts: library.prompts,
        recovery_required: transaction::pending(root),
    })
}

pub(crate) fn save(
    root: &Path,
    target: &Path,
    id: Option<&str>,
    draft: ClaudePromptDraft,
    expected_hash: &str,
) -> Result<ClaudePromptsView, String> {
    transaction::require_clear(root)?;
    draft.validate()?;
    let (mut library, _) = store::load(root)?;
    if let Some(id) = id {
        let prompt = library
            .prompts
            .iter_mut()
            .find(|prompt| prompt.id == id)
            .ok_or("Claude 提示词不存在")?;
        prompt.draft = draft;
    } else {
        library.prompts.push(ClaudePrompt {
            id: uuid::Uuid::new_v4().to_string(),
            draft,
        });
    }
    store::save(root, &library, expected_hash)?;
    list(root, target)
}

pub(crate) fn remove(
    root: &Path,
    target: &Path,
    id: &str,
    expected_hash: &str,
) -> Result<ClaudePromptsView, String> {
    transaction::require_clear(root)?;
    let (mut library, _) = store::load(root)?;
    if library
        .active
        .as_ref()
        .is_some_and(|active| active.id == id)
    {
        return Err("请先预览并停用此 Claude 提示词，再删除预设".into());
    }
    if !library.prompts.iter().any(|prompt| prompt.id == id) {
        return Err("Claude 提示词不存在".into());
    }
    library.prompts.retain(|prompt| prompt.id != id);
    store::save(root, &library, expected_hash)?;
    list(root, target)
}

pub(crate) fn reorder(
    root: &Path,
    target: &Path,
    ids: &[String],
    expected_hash: &str,
) -> Result<ClaudePromptsView, String> {
    transaction::require_clear(root)?;
    let (mut library, _) = store::load(root)?;
    if ids.len() != library.prompts.len()
        || ids.iter().collect::<std::collections::BTreeSet<_>>().len() != ids.len()
    {
        return Err("Claude 提示词排序必须包含每个预设且不能重复".into());
    }
    library.prompts = ids
        .iter()
        .map(|id| {
            library
                .prompts
                .iter()
                .find(|prompt| &prompt.id == id)
                .cloned()
                .ok_or_else(|| "Claude 提示词排序包含不存在的预设".to_string())
        })
        .collect::<Result<_, _>>()?;
    store::save(root, &library, expected_hash)?;
    list(root, target)
}

pub(crate) fn preview(
    root: &Path,
    target: &Path,
    id: Option<String>,
    expected_hash: &str,
) -> Result<ClaudePromptPreview, String> {
    transaction::require_clear(root)?;
    let (library, file_hash) = store::load(root)?;
    if file_hash != expected_hash {
        return Err("Claude 提示词库已改变，请重新读取".into());
    }
    let after = match &id {
        Some(id) => library
            .prompts
            .iter()
            .find(|prompt| &prompt.id == id)
            .ok_or("Claude 提示词不存在")?
            .draft
            .content
            .clone(),
        None if library.active.is_some() => String::new(),
        None => return Err("Claude 没有活动提示词，不会清空用户自有文档".into()),
    };
    let live = read_global_prompt_document(&FsIo, target, AppKind::Claude)
        .map_err(|error| error.to_string())?;
    Ok(ClaudePromptPreview {
        plan: ClaudePromptActivation {
            prompt_id: id,
            file_hash,
            live_hash: live.content_hash,
            live_exists: live.exists,
            rendered_hash: sha256_hex(&after),
        },
        before: live.content,
        after,
    })
}
