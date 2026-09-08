//! Durable pre-image of an explicitly confirmed active profile save.
use crate::local_state::LocalState;
use asb_core::{AppKind, ProviderFile};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ProfilePreimage {
    expected_after_hash: String,
    target: String,
    config_before_hash: String,
    config_after_hash: String,
    app: AppKind,
    file: ProviderFile,
}
fn path(state: &LocalState) -> std::path::PathBuf {
    state
        .root()
        .join("configuration")
        .join(crate::config_store::PROFILE_PREIMAGE_FILE)
}

pub(crate) fn save(
    state: &LocalState,
    app: AppKind,
    file: &ProviderFile,
    candidate: &asb_core::ProviderProfile,
    config_before_hash: &str,
    config_after_hash: &str,
) -> Result<(), String> {
    let candidate_file = ProviderFile::from_profile(candidate, file.position);
    let candidate_json =
        serde_json::to_string_pretty(&candidate_file).map_err(|_| "供应商候选无法序列化")?;
    let expected_after_hash = crate::config_store::content_revision(candidate_json.as_bytes());
    let text = serde_json::to_string(&ProfilePreimage {
        expected_after_hash,
        target: state.target(app)?.to_string_lossy().into_owned(),
        config_before_hash: config_before_hash.into(),
        config_after_hash: config_after_hash.into(),
        app,
        file: file.clone(),
    })
    .map_err(|_| "无法序列化供应商事务前像")?;
    let path = path(state);
    crate::config_store::write_json_atomic(&path, &text)?;
    Ok(())
}
pub(super) fn restore(
    state: &LocalState,
    profile_id: &str,
    expected_hash: &str,
) -> Result<(), String> {
    let record = state.configuration().find_provider_record(profile_id)?;
    let text = std::fs::read_to_string(path(state)).map_err(|_| "供应商事务前像不可读")?;
    let before: ProfilePreimage = serde_json::from_str(&text).map_err(|_| "供应商事务前像无效")?;
    if before.app != record.profile.app || before.file.id != profile_id {
        return Err("供应商事务前像身份不匹配".into());
    }
    if record.profile == before.file.clone().into_profile(before.app) {
        return Ok(());
    }
    if record.file_hash != expected_hash {
        return Err("待补偿供应商已发生额外修改，拒绝覆盖".into());
    }
    state
        .configuration()
        .overwrite_provider_file(before.app, before.file)?;
    Ok(())
}
pub(super) fn clear(state: &LocalState) -> Result<(), String> {
    match std::fs::remove_file(path(state)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("供应商事务已完成，但前像清理失败".into()),
    }
}

pub(super) fn validate_saved_revision(
    state: &LocalState,
    id: &str,
    hash: &str,
) -> Result<(), String> {
    let text = std::fs::read_to_string(path(state))
        .map_err(|_| "供应商确认版本前像不可读，不能自动采用当前版本")?;
    let before: ProfilePreimage =
        serde_json::from_str(&text).map_err(|_| "供应商确认版本前像无效")?;
    if before.file.id != id || before.expected_after_hash != hash {
        return Err("供应商版本既非保存前版本，也非已确认候选，拒绝恢复最新修改".into());
    }
    Ok(())
}

pub(super) fn validate_projection(
    state: &LocalState,
    app: AppKind,
    preview: &asb_switch::FilePreview,
) -> Result<(), String> {
    let text = std::fs::read_to_string(path(state)).map_err(|_| "供应商确认配置前像不可读")?;
    let before: ProfilePreimage =
        serde_json::from_str(&text).map_err(|_| "供应商确认配置前像无效")?;
    if before.app != app
        || std::path::Path::new(&before.target) != state.target(app)?
        || before.config_before_hash != preview.content_hash
        || before.config_after_hash != preview.rendered_hash
    {
        return Err("客户端配置或设置已偏离保存时确认的快照，拒绝重新解释待恢复保存".into());
    }
    Ok(())
}
