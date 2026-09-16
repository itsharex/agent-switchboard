//! Read-only Codex `[agents]` runtime settings for the unified client configuration transaction.
use std::{io::ErrorKind, path::Path};

use asb_core::{adapter::codex::{deprecated_subagent_keys, read_subagent_settings}, AppKind, CodexSubagentSettingsSnapshot};
use crate::{executor::{sha256_hex, SwitchError}, io::SwitchIo};

fn adapter_error(error: asb_core::adapter::AdapterError) -> SwitchError {
    SwitchError::PlanRejected { message: error.message, line: error.line }
}

pub fn read_codex_subagent_settings<Io: SwitchIo>(
    io: &Io,
    target: &Path,
) -> Result<CodexSubagentSettingsSnapshot, SwitchError> {
    let (content, file_exists) = match io.read_file(target) {
        Ok(content) => (content, true),
        Err(error) if error.kind() == ErrorKind::NotFound => (String::new(), false),
        Err(error) => return Err(SwitchError::ReadCurrent { message: error.to_string() }),
    };
    Ok(CodexSubagentSettingsSnapshot {
        app: AppKind::Codex,
        settings: read_subagent_settings(&content).map_err(adapter_error)?,
        config_hash: sha256_hex(&content),
        file_exists,
        deprecated_keys: deprecated_subagent_keys(&content).map_err(adapter_error)?,
    })
}
