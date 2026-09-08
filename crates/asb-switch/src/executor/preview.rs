//! Side-effect-free previews of both switch flavours plus the shared
//! current-file readers and paired hashing.

use asb_core::{AdapterError, AppKind, SwitchPlan};
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
    let (mut preview, rendered) = super::upgrade::candidate(&current, plan, backup_dir)?;
    if !target_existed {
        preview
            .warnings
            .push("配置文件尚不存在，确认后将创建新的用户级配置".to_string());
    }
    let rendered_hash = sha256_hex(&rendered);
    Ok(FilePreview {
        preview,
        content_hash,
        rendered_hash,
        content: display_content(app, &rendered),
    })
}

pub(super) fn plan_rejected(e: AdapterError) -> SwitchError {
    SwitchError::PlanRejected {
        message: e.message,
        line: e.line,
    }
}
