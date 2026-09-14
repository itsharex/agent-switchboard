//! Explicit recovery of the app-owned gateway state. Client files are never
//! changed here; their new capability must still be applied by the executor.
use super::*;
use std::fs::OpenOptions;
use std::io::Write;

pub(super) fn read_for_retry(path: &Path) -> StateLoad {
    match read_or_create_state(path) {
        StateLoad::Unusable(_) => match repair(path) {
            Ok(state) => StateLoad::Created(state),
            Err(error) => StateLoad::Unusable(error),
        },
        state => state,
    }
}

fn repair(path: &Path) -> Result<GatewayStateFile, String> {
    let original = fs::read(path)
        .map_err(|error| format!("无法读取待修复的网关状态，原文件保持不变：{error}"))?;
    if let Ok(state) = parse_state(&original) {
        return Ok(state);
    }
    let parent = path.parent().ok_or("网关状态目录无效")?;
    let backup = parent.join(format!("gateway.invalid.{}.json", Uuid::new_v4()));
    preserve(&backup, &original)?;
    let current = fs::read(path)
        .map_err(|error| format!("修复前无法复核网关状态，诊断副本已保留：{error}"))?;
    if current != original {
        return Err("网关状态在修复期间发生外部修改，已停止覆盖并保留诊断副本；请重试".into());
    }
    let mut state = fresh_state();
    if let Some(port) = saved_port(&original) {
        state.port = port;
    }
    write_state(path, &state)?;
    log::warn!(
        "网关状态已重建，旧状态保存在 {}；客户端须重新应用供应商",
        backup.display()
    );
    Ok(state)
}

fn preserve(path: &Path, original: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| format!("无法保存网关状态诊断副本，原文件保持不变：{error}"))?;
    file.write_all(original)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("网关状态诊断副本未完整写入，原文件保持不变：{error}"))?;
    let copied = fs::read(path)
        .map_err(|error| format!("无法验证网关状态诊断副本，原文件保持不变：{error}"))?;
    if copied != original {
        return Err("网关状态诊断副本内容不匹配，原文件保持不变".into());
    }
    Ok(())
}

/// A port is not an authority or a credential. Recover only its validated
/// public value; the old identity and route records are never adopted.
fn saved_port(bytes: &[u8]) -> Option<u16> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let port = u16::try_from(value.get("port")?.as_u64()?).ok()?;
    validate_custom_port(port).ok()?;
    Some(port)
}
