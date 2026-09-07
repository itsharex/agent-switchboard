//! Gateway controller lifecycle: start, observe, project, commit, shutdown.

use super::*;

impl GatewayController {
    pub(crate) fn start(local: &LocalState) -> Result<Self, String> {
        let state_path = local.gateway_state_path();
        let mut state = read_or_create_state(&state_path)?;
        let server = Arc::new(
            Server::http(("127.0.0.1", state.port))
                .map_err(|_| "无法启动本机协议网关：本机端口不可用".to_string())?,
        );
        let port = server
            .server_addr()
            .to_ip()
            .map(|address| address.port())
            .ok_or_else(|| "无法读取本机协议网关端口".to_string())?;
        if state.port == 0 {
            state.port = port;
            write_state(&state_path, &state)?;
        }
        let inner = Arc::new(GatewayInner {
            state_path,
            state: Mutex::new(state),
            routes: RwLock::new(BTreeMap::new()),
            activation_lock: Mutex::new(()),
            stopping: AtomicBool::new(false),
            base_url: format!("http://127.0.0.1:{port}"),
            metrics: Arc::new(GatewayMetrics::new()),
        });
        let controller = Self {
            inner: Arc::clone(&inner),
            server: Arc::clone(&server),
        };
        controller.rehydrate(local);
        thread::Builder::new()
            .name("asb-local-gateway".to_string())
            .spawn(move || server::serve(server, inner))
            .map_err(|_| "无法启动本机协议网关工作线程".to_string())?;
        Ok(controller)
    }

    /// Read-only snapshot of listener facts, active routes, and request
    /// telemetry for the status page. Never exposes tokens or upstream keys.
    pub(crate) fn observe(&self) -> Result<GatewayObservation, String> {
        let port = self
            .inner
            .state
            .lock()
            .map_err(|_| "本机协议网关状态锁不可用".to_string())?
            .port;
        let routes = self
            .inner
            .routes
            .read()
            .map_err(|_| "本机协议网关路由锁不可用".to_string())?
            .values()
            .map(|route| RouteObservation {
                app: route.app,
                profile_id: route.profile_id.clone(),
                upstream_protocol: route.upstream_protocol,
            })
            .collect();
        Ok(GatewayObservation {
            port,
            base_url: self.inner.base_url.clone(),
            routes,
            metrics: self.inner.metrics.snapshot(),
        })
    }

    /// Produces the client projection without mutating the gateway state.
    /// The caller can safely preview this result; its route token is bound to
    /// the route fingerprint, so editing a routing parameter invalidates
    /// stale previews while metadata edits keep the token stable.
    pub(crate) fn project(&self, plan: &SwitchPlan) -> Result<GatewayProjection, String> {
        validate_plan(&plan.profile, &plan.common).map_err(|error| error.to_string())?;
        if plan.profile.route_mode == RouteMode::Official || is_direct(&plan.profile) {
            return Ok(GatewayProjection {
                plan: plan.clone(),
                activation: GatewayActivation::Direct { app: plan.app() },
                warning: None,
            });
        }
        let route = self.route_for_profile(&plan.profile)?;
        let mut projected = SwitchPlan::through_gateway(plan.profile.clone(), plan.common.clone());
        projected.profile.base_url = Some(match route.app {
            AppKind::Codex => format!("{}/v1", self.inner.base_url),
            AppKind::Claude => self.inner.base_url.clone(),
        });
        // Both clients can authenticate to this loopback listener with a
        // bearer token. The original upstream protocol stays only on `route`.
        projected.profile.api_key = route.client_token.clone();
        let protocol = plan
            .profile
            .upstream_protocol
            .expect("validated custom profile has a protocol");
        let gateway_url = projected
            .profile
            .base_url
            .clone()
            .expect("routed projection has a gateway base url");
        let mut warning = format!(
            "该供应商的上游协议是 {}，与 {} 原生协议（{}）不同：切换后客户端配置中的服务地址会被改写为本机协议网关地址 {}（127.0.0.1 仅监听本机回环），请求由网关转换为 {} 格式并携带原 API 密钥转发到所填服务地址，原地址与密钥只保存在本应用内。退出应用前请先切换到直连或官方登录，否则客户端将无法请求。",
            protocol.label(),
            plan.profile.app.label(),
            UpstreamProtocol::native_for(plan.profile.app).label(),
            gateway_url,
            protocol.label(),
        );
        if plan.profile.app == AppKind::Codex && protocol != UpstreamProtocol::Responses {
            warning.push_str(
                " Codex 的网页搜索会在此路由中关闭，因为它是该上游无法无损承载的 Responses 服务端工具；client_metadata、prompt_cache_key、reasoning.summary=auto 与 reasoning.encrypted_content 也不会转发到上游或出现在转换后的输出。需要这些 Responses 能力时，请使用 Responses 上游。",
            );
        }
        Ok(GatewayProjection {
            plan: projected,
            activation: GatewayActivation::Routed(route),
            warning: Some(warning),
        })
    }

    /// Persists and publishes one post-switch route. Returning an error makes
    /// the surrounding switch transaction restore the client configuration.
    pub(crate) fn commit<F>(&self, projection: &GatewayProjection, persist: F) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        self.commit_activation(&projection.activation, persist)
    }

    /// Applies the gateway state that corresponds to a just-restored client
    /// configuration. The caller invokes this only from the restore
    /// executor's commit callback, so an unrecognized loopback configuration
    /// rolls the client files back instead of leaving them pointed at a dead
    /// local route.
    pub(crate) fn reconcile_restored<F>(
        &self,
        local: &LocalState,
        app: AppKind,
        persist: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let target = local.target(app)?;
        let text = match fs::read_to_string(&target) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Restoring a backup from before the client was configured is
                // valid. With no client file there cannot be a live loopback
                // route, so remove any in-memory and persisted activation in
                // the same executor callback that records this restore.
                return self.commit_activation(&GatewayActivation::Direct { app }, persist);
            }
            Err(_) => return Err("恢复后无法读取客户端配置，已拒绝更新本机协议网关".to_string()),
        };
        adapter::validate_syntax(app, &text)
            .map_err(|_| "恢复后客户端配置格式无效，已拒绝更新本机协议网关".to_string())?;
        let codex_auth = if app == AppKind::Codex {
            let auth_path = LocalState::codex_auth_path()?;
            match fs::read_to_string(auth_path) {
                Ok(auth) => Some(auth),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                Err(_) => {
                    return Err("恢复后无法读取 Codex 登录缓存，已拒绝更新本机协议网关".to_string())
                }
            }
        } else {
            None
        };
        let candidates = local
            .configuration()
            .list_providers()
            .map_err(|_| "恢复后无法读取供应商配置，已拒绝更新本机协议网关".to_string())?
            .into_iter()
            .filter_map(|record| {
                let profile = record.profile;
                (profile.app == app
                    && profile.route_mode == RouteMode::Custom
                    && !is_direct(&profile))
                .then_some(profile)
            })
            .map(|profile| self.route_for_profile(&profile))
            .collect::<Result<Vec<_>, _>>()?;
        let mut routes = Vec::new();
        for route in candidates {
            if self.route_matches_config(&route, &text, codex_auth.as_deref())? {
                routes.push(route);
            }
        }
        let activation = match routes.as_slice() {
            [route] => GatewayActivation::Routed(route.clone()),
            [] if self.points_to_gateway(app, &text) => {
                return Err(
                    "恢复的配置指向本机协议网关，但未找到匹配的供应商；已拒绝恢复".to_string(),
                )
            }
            [] => GatewayActivation::Direct { app },
            _ => return Err("恢复的配置匹配多个本机协议网关供应商，已拒绝恢复".to_string()),
        };
        self.commit_activation(&activation, persist)
    }

    /// Returns the active routed profile only when the current client files
    /// still hold the matching loopback endpoint and capability credential.
    /// This lets status and the tray distinguish a valid gateway activation
    /// from a stale in-memory route after external edits.
    pub(crate) fn active_profile_id(
        &self,
        app: AppKind,
        configuration: &str,
        codex_auth: Option<&str>,
    ) -> Result<Option<String>, String> {
        let route = self
            .inner
            .routes
            .read()
            .map_err(|_| "本机协议网关路由锁不可用".to_string())?
            .get(&app)
            .cloned();
        Ok(route
            .filter(|route| {
                self.route_matches_config(route, configuration, codex_auth)
                    .unwrap_or(false)
            })
            .map(|route| route.profile_id))
    }

    pub(crate) fn uses_profile(&self, profile_id: &str) -> bool {
        self.inner
            .routes
            .read()
            .map(|routes| routes.values().any(|route| route.profile_id == profile_id))
            .unwrap_or(true)
    }

    pub(crate) fn has_active_routes(&self) -> bool {
        self.inner
            .routes
            .read()
            .map(|routes| !routes.is_empty())
            .unwrap_or(true)
    }

    pub(crate) fn has_active_route_for(&self, app: AppKind) -> bool {
        self.inner
            .routes
            .read()
            .map(|routes| routes.contains_key(&app))
            .unwrap_or(true)
    }

    pub(crate) fn shutdown(&self) {
        self.inner.stopping.store(true, Ordering::Release);
        self.server.unblock();
    }
}
