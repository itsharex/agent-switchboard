use crate::local_state::LocalState;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Keep evidence of a temporary edit even when files return to the original
/// fingerprint before the next process poll. Access events are not changes.
pub(super) struct ConfigWatch {
    _watcher: RecommendedWatcher,
    problem: Arc<Mutex<Option<super::runner::ProbeAbort>>>,
}

impl ConfigWatch {
    pub(super) fn start(local: &LocalState) -> Result<Self, String> {
        let config = local.target(asb_core::AppKind::Codex)?;
        let mut paths = vec![config.clone(), config.with_file_name("auth.json")];
        if let Ok(text) = std::fs::read_to_string(&config) {
            if let Ok(doc) = text.parse::<toml_edit::DocumentMut>() {
                if let Some(path) = doc.get("model_catalog_json").and_then(|value| value.as_str()) {
                    let path = PathBuf::from(path);
                    paths.push(if path.is_absolute() { path } else {
                        config.parent().ok_or("Codex 配置路径无效")?.join(path)
                    });
                }
            }
        }
        let problem = Arc::new(Mutex::new(None));
        let sink = problem.clone();
        let watched_paths = paths.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let message = match event {
                Ok(event) if event.need_rescan() => Some(super::runner::ProbeAbort::Spawn(
                    "配置监视丢失了文件事件，无法确认本次配置一致性".into())),
                Ok(event) if !matches!(event.kind, EventKind::Access(_)) && event.paths.iter()
                    .any(|path| watched_paths.iter().any(|target| path == target || target.starts_with(path))) =>
                    Some(super::runner::ProbeAbort::ConfigChanged),
                Ok(_) => None,
                Err(error) => Some(super::runner::ProbeAbort::Spawn(format!("检测配置监视失败：{error}"))),
            };
            if let Some(message) = message { *sink.lock().expect("probe config watch") = Some(message); }
        }).map_err(|error| format!("无法监视检测配置：{error}"))?;
        let mut directories = Vec::new();
        for path in paths {
            let mut parent = path.parent().ok_or("检测配置路径无效")?.to_path_buf();
            while !parent.is_dir() {
                parent = parent.parent().ok_or("检测配置目录不存在")?.to_path_buf();
            }
            if directories.contains(&parent) { continue; }
            watcher.watch(&parent, RecursiveMode::NonRecursive)
                .map_err(|error| format!("无法监视检测配置目录：{error}"))?;
            directories.push(parent);
        }
        Ok(Self { _watcher: watcher, problem })
    }

    pub(super) fn check(&self) -> Result<(), super::runner::ProbeAbort> {
        match self.problem.lock().expect("probe config watch").clone() {
            Some(reason) => Err(reason),
            None => Ok(()),
        }
    }
}
