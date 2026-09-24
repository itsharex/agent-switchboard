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
    let rendered = adapter::render(&current, plan).map_err(plan_rejected)?;
    if !target_existed {
        preview.warnings.push(asb_core::contracts::LocalizedMessage::new(
            "warnings.preview.fileCreated",
            serde_json::json!({}),
            "配置文件尚不存在，确认后将创建新的用户级配置",
        ));
    }
    let rendered_hash = sha256_hex(&rendered);
    crate::codex_auth::validate_storage(&current, plan)?;
    let auth = crate::codex_auth::prepare(io, target, crate::codex_auth::expected_action(plan))?;
    Ok(FilePreview {
        preview,
        content_hash,
        rendered_hash,
        content: display_content(app, &rendered),
        auth_hash: auth.as_ref().map(|change| change.before_hash.clone()),
        auth_rendered_hash: auth.as_ref().map(|change| change.after_hash.clone()),
        auth_existed: auth.as_ref().map(|change| change.before_existed),
        auth_rendered_existed: auth.as_ref().map(|_| true),
    })
}

pub(super) fn plan_rejected(e: AdapterError) -> SwitchError {
    SwitchError::PlanRejected {
        message: e.message,
        line: e.line,
    }
}
