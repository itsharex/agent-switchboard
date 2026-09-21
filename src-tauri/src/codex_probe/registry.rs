use super::*;

// ------------------------------------------------------------------ registry

/// One probe slot: a second start is rejected while a probe runs. The slot
/// holds no readable state — status reads go to the ledger.
#[derive(Clone, Default)]
pub(crate) struct ProbeRegistry {
    slot: Arc<Mutex<Option<ActiveProbe>>>,
    shutting_down: Arc<AtomicBool>,
}

struct ActiveProbe {
    batch_id: String,
    /// Present while the worker thread may still be running.
    cancel: Option<Arc<AtomicBool>>,
    unsaved: Option<UnsavedProbe>,
}

impl ProbeRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(super) fn begin(&self, batch_id: &str) -> Result<Arc<AtomicBool>, String> {
        let mut slot = self.slot.lock().expect("probe registry");
        if self.shutting_down.load(Ordering::SeqCst) { return Err("应用正在退出，不能开始检测".into()); }
        if let Some(state) = slot.as_ref() {
            if state.cancel.is_some() {
                return Err("已有一次降智检测在运行，请先取消或等待完成".to_string());
            }
        }
        if slot.as_ref().is_some_and(|state| state.unsaved.is_some()) {
            return Err("上次检测结果尚未保存，请先重试保存".into());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *slot = Some(ActiveProbe {
            batch_id: batch_id.to_string(),
            cancel: Some(Arc::clone(&cancel)),
            unsaved: None,
        });
        Ok(cancel)
    }

    /// Signals the running worker, if any; the in-flight call is terminated.
    pub(crate) fn cancel(&self) -> bool {
        let mut slot = self.slot.lock().expect("probe registry");
        match slot.as_mut() {
            Some(state) => match state.cancel.as_ref() {
                Some(flag) => !flag.swap(true, Ordering::SeqCst),
                None => false,
            },
            None => false,
        }
    }

    pub(super) fn worker_finished(&self, batch_id: &str, unsaved: Option<UnsavedProbe>) {
        let mut slot = self.slot.lock().expect("probe registry");
        if let Some(state) = slot.as_mut() {
            if state.batch_id == batch_id && state.cancel.is_some() {
                state.cancel = None;
                state.unsaved = unsaved;
                if state.unsaved.is_none() {
                    *slot = None;
                }
            }
        }
    }

    /// Releases a claimed slot whose batch row could not be created — no
    /// call ran, so nothing else may reference it.
    pub(super) fn abandon(&self, batch_id: &str) {
        let mut slot = self.slot.lock().expect("probe registry");
        if matches!(slot.as_ref(),
            Some(state) if state.batch_id == batch_id && state.cancel.is_some())
        {
            *slot = None;
        }
    }

    pub(crate) fn unsaved(&self, batch_id: &str) -> Option<UnsavedProbe> {
        self.slot
            .lock()
            .expect("probe registry")
            .as_ref()
            .filter(|state| state.batch_id == batch_id)
            .and_then(|state| state.unsaved.clone())
    }

    /// Keep ownership while SQLite commits, including on failure. Starts and
    /// concurrent retries cannot reclaim the only copy of these results.
    pub(super) fn retry_save(&self, ledger: &ProbeLedger) -> Result<(), String> {
        let mut slot = self.slot.lock().expect("probe registry");
        if let Some(unsaved) = slot.as_mut().and_then(|state| state.unsaved.as_mut()) {
            if let Err(error) = ledger.finish_batch(&unsaved.batch_id, unsaved.terminal.0,
                unsaved.terminal.1.clone(), &unsaved.pending_runs) {
                unsaved.save_error = error.clone();
                return Err(error);
            }
            *slot = None;
        }
        Ok(())
    }

    pub(crate) fn resume_after_blocked_exit(&self) {
        self.shutting_down.store(false, Ordering::SeqCst);
    }

    /// Exit path: signal the worker (which terminates the child process),
    /// wait for it to settle before permitting a final save and exit.
    pub(crate) fn request_shutdown(&self, timeout: Duration) -> Result<(), String> {
        let batch_id = {
            let mut slot = self.slot.lock().expect("probe registry");
            self.shutting_down.store(true, Ordering::SeqCst);
            let Some(state) = slot.as_mut() else { return Ok(()); };
            if let Some(flag) = state.cancel.as_ref() {
                flag.store(true, Ordering::SeqCst);
            }
            state.batch_id.clone()
        };
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            let settled = {
                let slot = self.slot.lock().expect("probe registry");
                !matches!(slot.as_ref(),
                    Some(state) if state.batch_id == batch_id && state.cancel.is_some())
            };
            if settled {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Err("检测进程尚未结束，已取消退出，请稍后重试".into())
    }
}
