//! The universal connection store plus the Codex projection engine (P11).
//! Codex-owned: the store keeps shared connection facts only, the projection
//! rewrites exclusively the universal-owned profile fields, and every local
//! profile fact (name, connection options, parameters, notes) stays untouched.

use std::{
    fs,
    path::Path,
    sync::{Mutex, MutexGuard, OnceLock},
};

use asb_switch::{io::FsIo, sha256_hex, SwitchIo};
use serde::{Deserialize, Serialize};

use asb_core::contracts::{CodexProviderFile, CodexRouteMode, UniversalProvider};

fn lock() -> Result<MutexGuard<'static, ()>, String> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "通用连接锁不可用，请重启应用".into())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
struct UniversalFile {
    #[serde(default)]
    providers: Vec<UniversalProvider>,
}

fn store_path(root: &Path) -> std::path::PathBuf {
    root.join("universal/providers.json")
}

/// Loads every universal connection with the content-derived revision that
/// guards concurrent writes.
pub(crate) fn load(root: &Path) -> Result<(Vec<UniversalProvider>, String), String> {
    let _guard = lock()?;
    let raw = match fs::read_to_string(store_path(root)) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), sha256_hex("[]")))
        }
        Err(_) => return Err("无法读取通用连接库".into()),
    };
    let file: UniversalFile =
        serde_json::from_str(&raw).map_err(|_| "通用连接库格式无效，请从备份恢复".to_string())?;
    Ok((file.providers, sha256_hex(&raw)))
}

/// Replaces the stored list only when `expected` still matches; returns the
/// new revision. The write is atomic and the revision stays content-derived.
pub(crate) fn save(
    root: &Path,
    providers: Vec<UniversalProvider>,
    expected: &str,
) -> Result<String, String> {
    let _guard = lock()?;
    let text = serde_json::to_string_pretty(&UniversalFile { providers })
        .map_err(|_| "通用连接库无法序列化")?;
    if load_unlocked(root)? != expected {
        return Err("通用连接库已变化，请刷新后重试".into());
    }
    let path = store_path(root);
    let parent = path.parent().ok_or("通用连接库路径无效")?;
    fs::create_dir_all(parent).map_err(|_| "无法创建通用连接目录")?;
    let temporary = parent.join(format!("providers.{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        FsIo.write_new_file(&temporary, &text)
            .map_err(|_| "无法写入通用连接临时文件")?;
        fs::rename(&temporary, &path).map_err(|_| "无法原子替换通用连接库")?;
        FsIo.sync_dir(parent)
            .map_err(|_| "通用连接库已保存，但目录同步失败")?;
        Ok::<_, String>(sha256_hex(&text))
    })();
    if result.is_err() && temporary.exists() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

/// CAS check without re-entrancy through the public `load` (the caller holds
/// the store lock).
fn load_unlocked(root: &Path) -> Result<String, String> {
    match fs::read_to_string(store_path(root)) {
        Ok(raw) => Ok(sha256_hex(&raw)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(sha256_hex("[]")),
        Err(_) => Err("无法读取通用连接库".into()),
    }
}

/// Rewrites the universal-owned profile facts and leaves everything else
/// alone. Returns warnings; the caller compares the file for a no-op sync.
pub(crate) fn project_universal_into_codex(
    universal: &UniversalProvider,
    file: &mut CodexProviderFile,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let profile = &mut file.profile;
    profile.endpoint = universal.endpoint.clone();
    profile.api_key = universal.api_key.clone();
    profile.upstream = universal.upstream;
    profile.authentication = universal.authentication;
    profile.catalog = universal.catalog.clone();
    if profile
        .catalog
        .iter()
        .any(|entry| entry.id == universal.default_model)
    {
        profile.default_model = universal.default_model.clone();
    } else {
        warnings.push("通用连接的默认模型不在目录中，已保留本端默认模型".into());
    }
    let before = profile.model_routes.len();
    profile.model_routes.retain(|route| {
        profile
            .catalog
            .iter()
            .any(|entry| entry.id == route.upstream_model)
    });
    if profile.model_routes.len() != before {
        warnings.push(format!(
            "{} 条模型别名因通用目录变更已移除",
            before - profile.model_routes.len()
        ));
    }
    // Connection facts changed upstream; the stored route mode must follow
    // the same single owner the profile editor uses.
    profile.route_mode = CodexRouteMode::for_connection(
        profile.upstream,
        &profile.connection,
        profile.authentication,
    );
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use asb_core::contracts::{CodexUpstream, UniversalProviderInput};

    fn provider_input(name: &str, endpoint: &str, models: &[&str]) -> UniversalProviderInput {
        serde_json::from_value(serde_json::json!({
            "name": name,
            "endpoint": endpoint,
            "upstream": "responses",
            "apiKey": "universal-key",
            "models": models,
        }))
        .unwrap()
    }

    fn sample(id: &str) -> UniversalProvider {
        provider_input("Relay", "https://relay.example", &["gpt-5.2", "gpt-5.1"])
            .into_provider(id.into())
            .unwrap()
    }

    #[test]
    fn store_roundtrips_with_cas_and_rejects_stale_revisions() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        let (providers, revision) = load(root).unwrap();
        assert!(providers.is_empty());

        let next = save(root, vec![sample("u1")], &revision).unwrap();
        assert_ne!(next, revision);
        let (providers, revision) = load(root).unwrap();
        assert_eq!(providers.len(), 1);

        assert!(save(root, Vec::new(), &revision.replace('a', "b")).is_err());
        let (providers, _) = load(root).unwrap();
        assert_eq!(providers.len(), 1, "stale write must not land");
    }

    #[test]
    fn projection_rewrites_universal_facts_and_preserves_local_ownership() {
        let mut universal = provider_input("Relay", "https://relay.example", &["gpt-5.2"])
            .into_provider("u1".into())
            .unwrap();
        // The local profile predates the upstream rotation below.
        let mut file = universal.to_codex_provider().into_file("p1".into(), 0);
        // Local facts that sync must never touch.
        file.profile.name = "本地改名".into();
        file.profile
            .model_routes
            .push(asb_core::contracts::CodexModelRoute {
                client_model: "fast".into(),
                upstream_model: "gpt-5.2".into(),
            });
        let stale_route = asb_core::contracts::CodexModelRoute {
            client_model: "gone".into(),
            upstream_model: "retired-model".into(),
        };
        file.profile.model_routes.push(stale_route);
        file.notes = Some("本地备注".into());
        // Upstream rotations: a new key plus a default model that is not in
        // the shared catalog.
        universal.api_key = "rotated-key".into();
        universal.default_model = "gpt-9".into();

        let warnings = project_universal_into_codex(&universal, &mut file);
        assert!(warnings.iter().any(|warning| warning.contains("默认模型")));
        assert!(warnings.iter().any(|warning| warning.contains("模型别名")));
        assert_eq!(file.profile.name, "本地改名");
        assert_eq!(file.profile.api_key, "rotated-key");
        assert_eq!(file.profile.default_model, "gpt-5.2", "本端默认模型保留");
        assert_eq!(file.profile.model_routes.len(), 1);
        assert_eq!(file.notes.as_deref(), Some("本地备注"));
        // Upstream stayed Responses: the route mode follows the projected
        // facts through the single owner.
        assert_eq!(
            file.profile.route_mode,
            CodexRouteMode::for_connection(
                file.profile.upstream,
                &file.profile.connection,
                file.profile.authentication,
            )
        );
    }

    #[test]
    fn projection_is_idempotent() {
        let universal = sample("u1");
        let mut file = universal.to_codex_provider().into_file("p1".into(), 0);
        let warnings = project_universal_into_codex(&universal, &mut file);
        assert!(warnings.is_empty());
        let once = file.clone();
        let warnings = project_universal_into_codex(&universal, &mut file);
        assert!(warnings.is_empty());
        assert_eq!(once, file, "second projection must not change the file");
    }
}
