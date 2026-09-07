//! Gateway listener startup, binding, and retry lifecycle.

use super::*;

impl GatewayController {
    /// Resolves state, recovers a port transaction, binds, and rehydrates.
    /// Every failure becomes an observable controller state so the window can
    /// still open with an actionable repair surface.
    pub(crate) fn start(local: &LocalState) -> Self {
        let state_path = local.gateway_state_path();
        let (mut state, unusable) = match read_or_create_state(&state_path) {
            StateLoad::Ready(state) | StateLoad::Created(state) | StateLoad::Quarantined(state) => {
                (state, None)
            }
            StateLoad::Unusable(detail) => {
                log::warn!("本机协议网关状态不可用：{detail}");
                (fresh_state(), Some(detail))
            }
        };
        let blocked = if unusable.is_none() {
            match port_change::recover_pending(local, &state_path, &mut state) {
                Ok(blocked) => blocked,
                Err(reason) => {
                    log::warn!("本机协议网关端口修改恢复失败：{reason}");
                    Some(port_change::BlockedPortChange {
                        from_port: state.port,
                        to_port: state.port,
                        apps: Vec::new(),
                        reason,
                    })
                }
            }
        } else {
            None
        };
        let state_available = unusable.is_none();
        let controller = Self {
            inner: Arc::new(GatewayInner {
                state_path,
                state: Mutex::new(state),
                routes: RwLock::new(BTreeMap::new()),
                activation_lock: Mutex::new(()),
                listener: RwLock::new(ListenerState::Failed(Box::new(GatewayFailureReport {
                    port: 0,
                    kind: GatewayFailureKind::StateUnusable,
                    os_code: None,
                    message: unusable.unwrap_or_else(|| "本机协议网关正在启动".to_string()),
                    process: None,
                }))),
                maintenance: AtomicBool::new(false),
                inflight: AtomicUsize::new(0),
                stopping: AtomicBool::new(false),
                state_available: AtomicBool::new(state_available),
                blocked_recovery: Mutex::new(blocked),
                metrics: Arc::new(GatewayMetrics::new()),
            }),
        };
        if controller.inner.state_available.load(Ordering::Acquire) {
            controller.bind_configured_port();
            controller.rehydrate(local);
        }
        controller
    }

    /// Binds the configured port. Tests may initially use zero so the OS
    /// assigns a port; production state always carries an explicit port.
    fn bind_configured_port(&self) {
        let state_path = self.inner.state_path.clone();
        let mut port = self.configured_port();
        let server = match Server::http(("127.0.0.1", port)) {
            Ok(server) => server,
            Err(error) => {
                let report = bind_failure_report(port, error);
                log::warn!("{}", report.message);
                self.set_listener(ListenerState::Failed(Box::new(report)));
                return;
            }
        };
        let bound_port = server
            .server_addr()
            .to_ip()
            .map(|address| address.port())
            .unwrap_or(port);
        if bound_port != port {
            if port != 0 {
                self.set_listener(ListenerState::Failed(Box::new(GatewayFailureReport {
                    port,
                    kind: GatewayFailureKind::SystemRejected,
                    os_code: None,
                    message: format!(
                        "无法监听 127.0.0.1:{port}：监听层返回了其他端口 {bound_port}"
                    ),
                    process: None,
                })));
                return;
            }
            port = bound_port;
            let mut state = match self.inner.state.lock() {
                Ok(state) => state,
                Err(_) => {
                    self.set_listener(ListenerState::Failed(Box::new(test_failure_report(
                        port,
                        "本机协议网关状态锁不可用",
                    ))));
                    return;
                }
            };
            state.port = port;
            if let Err(error) = write_state(&state_path, &state) {
                log::warn!("无法持久化本机协议网关端口：{error}");
            }
        }
        let listener = BoundListener::from_server(server);
        if let Err(error) = self.spawn_serve(listener.clone()) {
            log::warn!("{error}");
            self.set_listener(ListenerState::Failed(Box::new(GatewayFailureReport {
                port,
                kind: GatewayFailureKind::SystemRejected,
                os_code: None,
                message: error,
                process: None,
            })));
            return;
        }
        self.set_listener(ListenerState::Listening { listener });
    }

    /// Starts the serving loop and waits until its client and workers are
    /// initialized; a bound but inert socket is never published as healthy.
    pub(crate) fn spawn_serve(&self, listener: BoundListener) -> Result<(), String> {
        let inner = Arc::clone(&self.inner);
        let (ready_sender, ready_receiver) = std::sync::mpsc::sync_channel(1);
        let worker_listener = listener.clone();
        if thread::Builder::new()
            .name("asb-local-gateway".to_string())
            .spawn(move || server::serve(worker_listener, inner, ready_sender))
            .is_err()
        {
            listener.stop();
            return Err("无法启动本机协议网关工作线程".to_string());
        }
        match ready_receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                listener.stop();
                Err(error)
            }
            Err(_) => {
                listener.stop();
                Err("无法确认本机协议网关工作线程已就绪".to_string())
            }
        }
    }

    /// Brings the listener up on the configured port. An existing listener is
    /// left untouched; a failure is reported in the returned observation.
    pub(crate) fn retry_bind(&self, local: &LocalState) -> GatewayObservation {
        if !self.inner.state_available.load(Ordering::Acquire) && !self.reload_state(local) {
            return self.observe(local);
        }
        if self.listening().is_none() {
            self.bind_configured_port();
            self.rehydrate(local);
        }
        self.observe(local)
    }

    /// A discarded recovery keeps whatever client files survived. If a
    /// pre-commit failure left the saved endpoint ahead of the live listener,
    /// make the application-owned port fact follow the listener before
    /// normal operations resume; unmatched clients then remain visibly in
    /// repair instead of being reported as healthy against a dead address.
    pub(crate) fn synchronize_port_to_listener(&self) -> Result<(), String> {
        let Some(listener) = self.listening() else {
            return Ok(());
        };
        if listener.port() != self.configured_port() {
            self.persist_port(listener.port())?;
        }
        Ok(())
    }

    fn reload_state(&self, local: &LocalState) -> bool {
        let state_path = local.gateway_state_path();
        let mut state = match read_or_create_state(&state_path) {
            StateLoad::Ready(state) | StateLoad::Created(state) | StateLoad::Quarantined(state) => {
                state
            }
            StateLoad::Unusable(detail) => {
                self.set_listener(ListenerState::Failed(Box::new(GatewayFailureReport {
                    port: self.configured_port(),
                    kind: GatewayFailureKind::StateUnusable,
                    os_code: None,
                    message: detail,
                    process: None,
                })));
                return false;
            }
        };
        let blocked =
            port_change::recover_pending(local, &state_path, &mut state).unwrap_or_else(|reason| {
                Some(port_change::BlockedPortChange {
                    from_port: state.port,
                    to_port: state.port,
                    apps: Vec::new(),
                    reason,
                })
            });
        let Ok(mut stored) = self.inner.state.lock() else {
            return false;
        };
        *stored = state;
        drop(stored);
        if let Ok(mut routes) = self.inner.routes.write() {
            routes.clear();
        } else {
            return false;
        }
        if let Ok(mut blocked_slot) = self.inner.blocked_recovery.lock() {
            *blocked_slot = blocked;
        } else {
            return false;
        }
        self.inner.state_available.store(true, Ordering::Release);
        true
    }
}
