//! Durable application-side intent journal helpers.
use super::*;

pub(super) fn path(state: &LocalState) -> PathBuf {
    state
        .root()
        .join("configuration")
        .join(crate::config_store::SWITCH_INTENT_FILE)
}

pub(super) fn load(state: &LocalState) -> Result<Option<SwitchIntent>, String> {
    match std::fs::read_to_string(path(state)) {
        Ok(text) => serde_json::from_str(&text).map(Some).map_err(|_| {
            format!(
                "配置事务意图格式无效，请保留并核对 {}",
                path(state).display()
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(format!(
            "配置事务意图不可读，请检查 {} 的权限",
            path(state).display()
        )),
    }
}

pub(super) fn clear(state: &LocalState) -> Result<(), String> {
    let journal = path(state);
    match std::fs::remove_file(&journal) {
        Ok(()) => FsIo
            .sync_dir(journal.parent().expect("journal directory"))
            .map_err(|_| "配置事务清理无法持久化".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("配置事务已处理但意图清理失败，请重试恢复".into()),
    }
}

pub(super) fn read_target(target: &Path, app: AppKind) -> Result<(String, bool), String> {
    match std::fs::read_to_string(target) {
        Ok(text) => Ok((text, true)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((
            if app == AppKind::Claude {
                "{}".into()
            } else {
                String::new()
            },
            false,
        )),
        Err(_) => Err("配置事务目标不可读".into()),
    }
}

pub(super) fn error(message: impl Into<String>) -> CommandError {
    CommandError::new("config-recovery-required", message)
}
