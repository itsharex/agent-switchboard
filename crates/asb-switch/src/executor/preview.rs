//! Side-effect-free previews of both switch flavours plus the shared
//! current-file readers and paired hashing.

use asb_core::{adapter, AdapterError, AppKind, SwitchPlan};
use std::io::ErrorKind;
use std::path::Path;

use super::{sha256_hex, FilePreview, SwitchError};
use crate::display::display_content;
use crate::io::SwitchIo;

fn empty_configuration(app: AppKind) -> &'static str {
    match app {
        AppKind::Codex => "",
        AppKind::Claude => "{}",
    }
}

pub(crate) fn read_current_or_empty<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    app: AppKind,
) -> Result<(String, bool), std::io::Error> {
    match io.read_file(target) {
        Ok(text) => Ok((text, true)),
        Err(error) if error.kind() == ErrorKind::NotFound => {
            Ok((empty_configuration(app).to_string(), false))
        }
        Err(error) => Err(error),
    }
}

pub(super) fn read_auth_or_empty<Io: SwitchIo>(
    io: &Io,
    target: &Path,
) -> Result<(String, bool), std::io::Error> {
    match io.read_file(target) {
        Ok(text) => Ok((text, true)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok((String::new(), false)),
        Err(error) => Err(error),
    }
}

pub(super) fn paired_hash(config_hash: &str, auth_hash: &str) -> String {
    sha256_hex(&format!("config:{config_hash}\nauth:{auth_hash}"))
}

/// Reads the target and produces the side-effect-free preview plus the
/// content hash the executor will later verify.
pub fn read_preview<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    plan: &SwitchPlan,
    backup_dir: &str,
) -> Result<FilePreview, SwitchError> {
    let app = plan.app();
    let (current, target_existed) =
        read_current_or_empty(io, target, app).map_err(|e| SwitchError::ReadCurrent {
            message: e.to_string(),
        })?;
    let content_hash = sha256_hex(&current);
    let mut preview = adapter::preview(&current, plan, backup_dir).map_err(plan_rejected)?;
    if !target_existed {
        preview
            .warnings
            .push("配置文件尚不存在，确认后将创建新的用户级配置".to_string());
    }
    let rendered = adapter::render(&current, plan).map_err(plan_rejected)?;
    let rendered_hash = sha256_hex(&rendered);
    Ok(FilePreview {
        preview,
        content_hash,
        rendered_hash,
        content: display_content(app, &rendered),
    })
}

/// Produces the normal redacted configuration preview plus the credential
/// changes needed by Codex's built-in `openai` provider. The exposed hashes
/// cover both files, so a credential-cache edit after preview is a conflict.
pub fn read_codex_preview<Io: SwitchIo>(
    io: &Io,
    target: &Path,
    auth_target: &Path,
    plan: &SwitchPlan,
    backup_dir: &str,
) -> Result<FilePreview, SwitchError> {
    if plan.app() != AppKind::Codex {
        return Err(SwitchError::PlanRejected {
            message: "Codex 登录缓存只能由 Codex 切换使用".to_string(),
            line: None,
        });
    }
    let mut preview = read_preview(io, target, plan, backup_dir)?;
    let (auth_current, auth_existed) =
        read_auth_or_empty(io, auth_target).map_err(|error| SwitchError::ReadCurrent {
            message: error.to_string(),
        })?;
    let auth_changes = adapter::preview_codex_auth(&auth_current, plan).map_err(plan_rejected)?;
    let auth_rendered = adapter::render_codex_auth(&auth_current, plan).map_err(plan_rejected)?;
    preview.preview.changes.extend(auth_changes);
    if !auth_existed && auth_rendered != auth_current {
        preview
            .preview
            .warnings
            .push("Codex API-key 登录缓存尚不存在，确认后将创建 auth.json".to_string());
    }
    preview.content_hash = paired_hash(&preview.content_hash, &sha256_hex(&auth_current));
    preview.rendered_hash = paired_hash(&preview.rendered_hash, &sha256_hex(&auth_rendered));
    Ok(preview)
}

pub(super) fn plan_rejected(e: AdapterError) -> SwitchError {
    SwitchError::PlanRejected {
        message: e.message,
        line: e.line,
    }
}
