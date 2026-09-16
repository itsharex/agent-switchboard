//! Emits one renderer event when a real client configuration file changes.
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        mpsc::{self, Receiver},
        Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

const CHANGE_EVENT: &str = "client-config-changed";
const DEBOUNCE: Duration = Duration::from_millis(120);

/// Keeps the platform watcher alive for the desktop application's lifetime.
pub struct ClientConfigWatcher {
    _watcher: Mutex<RecommendedWatcher>,
}

impl ClientConfigWatcher {
    pub fn start(app: AppHandle, targets: [PathBuf; 2]) -> Result<Self, notify::Error> {
        let (sender, receiver) = mpsc::channel();
        let mut watcher = notify::recommended_watcher(move |event| {
            let _ = sender.send(event);
        })?;

        for (directory, mode) in watch_roots(&targets) {
            watcher.watch(&directory, mode)?;
        }
        thread::spawn(move || relay_changes(app, receiver, targets));
        Ok(Self {
            _watcher: Mutex::new(watcher),
        })
    }
}

fn watch_roots(targets: &[PathBuf; 2]) -> BTreeMap<PathBuf, RecursiveMode> {
    let mut roots = BTreeMap::new();
    for target in targets {
        let parent = target.parent().expect("client configuration path has a parent");
        let mut root = parent;
        while !root.exists() {
            root = root.parent().expect("client configuration path has an existing ancestor");
        }
        let mode = if root == parent {
            RecursiveMode::NonRecursive
        } else {
            RecursiveMode::Recursive
        };
        roots
            .entry(root.to_path_buf())
            .and_modify(|current| {
                if matches!(mode, RecursiveMode::Recursive) {
                    *current = RecursiveMode::Recursive;
                }
            })
            .or_insert(mode);
    }
    roots
}

fn relay_changes(
    app: AppHandle,
    receiver: Receiver<notify::Result<Event>>,
    targets: [PathBuf; 2],
) {
    while let Ok(event) = receiver.recv() {
        match event {
            Ok(event) if touches_target(&event, &targets) => {
                drain_burst(&receiver, &targets);
                if let Err(error) = app.emit(CHANGE_EVENT, ()) {
                    log::warn!("客户端配置变更通知失败: {error}");
                }
            }
            Ok(_) => {}
            Err(error) => log::warn!("客户端配置文件监听失败: {error}"),
        }
    }
}

fn drain_burst(receiver: &Receiver<notify::Result<Event>>, targets: &[PathBuf; 2]) {
    let mut deadline = Instant::now() + DEBOUNCE;
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return;
        }
        match receiver.recv_timeout(remaining) {
            Ok(Ok(event)) if touches_target(&event, targets) => {
                deadline = Instant::now() + DEBOUNCE;
            }
            Ok(Ok(_)) => {}
            Ok(Err(error)) => log::warn!("客户端配置文件监听失败: {error}"),
            Err(_) => return,
        }
    }
}

fn touches_target(event: &Event, targets: &[PathBuf; 2]) -> bool {
    event
        .paths
        .iter()
        .any(|path| targets.iter().any(|target| same_path(path, target)))
}

fn same_path(left: &Path, right: &Path) -> bool {
    left == right
}
