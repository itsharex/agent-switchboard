//! Gateway observations and client projections.

use super::*;

impl GatewayController {
    /// Projects a current-format Codex profile through the local capability
    /// gateway. Official Codex has no profile and therefore never enters this
    /// path.
    pub(crate) fn project_codex(
        &self,
        file: &asb_core::contracts::CodexProviderFile,
        client_settings: asb_core::contracts::SettingsValues,
    ) -> Result<GatewayProjection, String> {
        file.validate()?;
        if self.blocked_recovery().is_some() {
            return Err("存在未完成的端口修改恢复，无法切换 Codex 供应商".to_string());
        }
        let base_url = self
            .listening_base_url()
            .ok_or_else(|| "本机协议网关当前未在监听，无法写入 Codex 第三方配置".to_string())?;
        let route = self.route_for_codex_file(file)?;
        let profile = file.client_projection().into_profile(AppKind::Codex);
        let catalog = CodexCatalogProjection {
            file_name: format!(
                "agent-switchboard-codex-{}-{}.json",
                file.profile.id,
                &route.fingerprint[..16]
            ),
            content: file.model_catalog_json()?,
        };
        let plan = SwitchPlan::through_gateway(
            profile,
            client_settings,
            route.client_endpoint(&base_url),
            route.client_token.clone(),
        )
        .with_codex_model_catalog(catalog.file_name.clone());
        asb_core::validate_plan(&plan.profile, &plan.client_settings)
            .map_err(|error| error.to_string())?;
        let warning = if file.profile.request_mode
            == asb_core::contracts::ResponsesRequestMode::Minimal
        {
            "Codex 第三方模型请求将经过本机协议网关；最小模式省略 reasoning、service_tier、store、include、metadata 与客户端缓存扩展字段，非空 previous_response_id 必须改为完整 input".to_string()
        } else {
            "Codex 第三方模型请求将经过本机协议网关".to_string()
        };
        Ok(GatewayProjection {
            plan,
            activation: GatewayActivation::Routed(route),
            warning: Some(warning),
            codex_catalog: Some(catalog),
        })
    }

    /// Read-only snapshot of the configured port, the live listener, active
    /// routes, and request telemetry for the status page. Never exposes
    /// tokens or upstream keys.
    pub(crate) fn observe(&self, local: &LocalState) -> GatewayObservation {
        let configured_port = self.configured_port();
        let listening = self.listening();
        let failure = if listening.is_some() {
            None
        } else {
            match self.inner.listener.read() {
                Ok(listener) => match &*listener {
                    ListenerState::Failed(report) => Some((**report).clone()),
                    ListenerState::Listening { .. } => None,
                },
                Err(_) => Some(test_failure_report(
                    configured_port,
                    "本机协议网关状态锁不可用",
                )),
            }
        };
        let blocked = self.blocked_recovery();
        let status = if blocked.is_some() {
            GatewayStatusKind::RecoveryBlocked
        } else if listening
            .as_ref()
            .is_some_and(|listener| listener.port() != configured_port)
        {
            GatewayStatusKind::NeedsRepair
        } else if self.has_unreconciled_gateway_files(local) {
            GatewayStatusKind::NeedsRepair
        } else if listening.is_none() {
            match failure.as_ref().map(|report| report.kind) {
                Some(GatewayFailureKind::PortInUse) => GatewayStatusKind::PortConflict,
                Some(GatewayFailureKind::StateUnusable) => GatewayStatusKind::NeedsRepair,
                _ => GatewayStatusKind::BindRejected,
            }
        } else if self.has_active_routes() {
            GatewayStatusKind::Running
        } else {
            GatewayStatusKind::Standby
        };
        let routes = self
            .inner
            .routes
            .read()
            .map(|routes| {
                routes
                    .values()
                    .map(|route| RouteObservation {
                        app: route.app,
                        profile_id: route.profile_id.clone(),
                        upstream_protocol: route.upstream_protocol,
                    })
                    .collect()
            })
            .unwrap_or_default();
        let repair_reason = (status == GatewayStatusKind::NeedsRepair)
            .then(|| {
                self.inner
                    .repair_reason
                    .lock()
                    .ok()
                    .and_then(|reason| reason.clone())
            })
            .flatten();
        let listening_port = listening.as_ref().map(BoundListener::port);
        GatewayObservation {
            configured_port,
            listening_port,
            base_url: listening_port.map(|port| format!("http://127.0.0.1:{port}")),
            status,
            failure,
            repair_reason,
            blocked_recovery: blocked,
            routes,
            metrics: self.inner.metrics.snapshot(),
        }
    }

    /// Produces the client projection without mutating the gateway state.
    /// Codex projections use one installation-scoped capability while their
    /// active route revision validates the selected provider. Claude retains
    /// its existing per-route Bearer capability.
    ///
    /// Routed projections are rejected while the listener is down or a
    /// port-change recovery is blocked: a gateway-dependent client write must
    /// never land against an address nothing serves.
    pub(crate) fn project(&self, plan: &SwitchPlan) -> Result<GatewayProjection, String> {
        validate_plan(&plan.profile, &plan.client_settings).map_err(|error| error.to_string())?;
        if plan.profile.route_mode == RouteMode::Official || is_direct(&plan.profile) {
            return Ok(GatewayProjection {
                plan: plan.clone(),
                activation: GatewayActivation::Direct { app: plan.app() },
                warning: None,
                codex_catalog: None,
            });
        }
        if self.blocked_recovery().is_some() {
            return Err(
                "存在未完成的端口修改恢复，已暂停经本机协议网关的切换；请先在网关页处理恢复状态"
                    .to_string(),
            );
        }
        let base_url = self.listening_base_url().ok_or_else(|| {
            "本机协议网关当前未在监听，无法写入经网关转换的客户端配置；请先在网关页恢复监听"
                .to_string()
        })?;
        let route = self.route_for_profile(&plan.profile)?;
        let projected = SwitchPlan::through_gateway(
            plan.profile.clone(),
            plan.client_settings.clone(),
            route.client_endpoint(&base_url),
            route.client_token.clone(),
        );
        let warning = projection_warning(plan, &projected);
        Ok(GatewayProjection {
            plan: projected,
            activation: GatewayActivation::Routed(route),
            warning: Some(warning),
            codex_catalog: None,
        })
    }

    /// Status compares the full current parameter intent even when the
    /// gateway's routing identity stayed stable or its listener is offline.
    pub(crate) fn active_route_projection(
        &self,
        plan: &SwitchPlan,
    ) -> Result<Option<SwitchPlan>, String> {
        let route = self
            .inner
            .routes
            .read()
            .map_err(|_| "本机协议网关路由锁不可用".to_string())?
            .get(&plan.app())
            .cloned();
        let Some(route) = route else {
            return Ok(None);
        };
        if route.profile_id != plan.profile.id
            || route.fingerprint != route_fingerprint(&plan.profile)?
        {
            return Ok(None);
        }
        Ok(Some(SwitchPlan::through_gateway(
            plan.profile.clone(),
            plan.client_settings.clone(),
            route.client_endpoint(&self.configured_base_url()),
            route.client_token.clone(),
        )))
    }

    /// Recreates the exact current Codex client projection from a specialized
    /// provider file only when that file still owns the active route revision.
    /// Status code uses this to include the catalog pointer and every managed
    /// Codex setting in its match test.
    pub(crate) fn active_codex_projection(
        &self,
        file: &asb_core::contracts::CodexProviderFile,
        client_settings: asb_core::contracts::SettingsValues,
    ) -> Result<Option<SwitchPlan>, String> {
        let route = self
            .inner
            .routes
            .read()
            .map_err(|_| "本机协议网关路由锁不可用".to_string())?
            .get(&AppKind::Codex)
            .cloned();
        let Some(route) = route else {
            return Ok(None);
        };
        let revision = codex_route_fingerprint(file)?;
        if route.profile_id != file.profile.id || route.fingerprint != revision {
            return Ok(None);
        }
        let profile = file.client_projection().into_profile(AppKind::Codex);
        Ok(Some(
            SwitchPlan::through_gateway(
                profile,
                client_settings,
                route.client_endpoint(&self.configured_base_url()),
                route.client_token,
            )
            .with_codex_model_catalog(format!(
                "agent-switchboard-codex-{}-{}.json",
                file.profile.id,
                &revision[..16]
            )),
        ))
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
        let candidates = match app {
            AppKind::Codex => local
                .configuration()
                .list_codex_providers()
                .map_err(|_| "恢复后无法读取 Codex 供应商配置，已拒绝更新本机协议网关".to_string())?
                .into_iter()
                .map(|record| {
                    local
                        .configuration()
                        .find_codex_provider_file(&record.profile.id)
                        .map_err(|_| {
                            "恢复后无法读取 Codex 供应商档案，已拒绝更新本机协议网关".to_string()
                        })
                        .and_then(|file| self.route_for_codex_file(&file))
                })
                .collect::<Result<Vec<_>, _>>()?,
            AppKind::Claude => local
                .configuration()
                .list_providers()
                .map_err(|_| "恢复后无法读取供应商配置，已拒绝更新本机协议网关".to_string())?
                .into_iter()
                .filter_map(|record| {
                    let profile = record.profile;
                    (profile.route_mode == RouteMode::Custom && !is_direct(&profile))
                        .then_some(profile)
                })
                .map(|profile| self.route_for_profile(&profile))
                .collect::<Result<Vec<_>, _>>()?,
        };
        let mut routes = Vec::new();
        for route in candidates {
            if self.route_matches_config(&route, &text)? {
                routes.push(route);
            }
        }
        let activation = match routes.as_slice() {
            [route] => GatewayActivation::Routed(route.clone()),
            [] if routing::config_points_at_gateway(app, &text) => {
                return Err(
                    "恢复的配置指向本机协议网关，但未找到匹配的供应商；已拒绝恢复。请重新应用该供应商以更新地址与凭据".to_string(),
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
                self.route_matches_config(route, configuration)
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

    /// Whether any client still depends on this gateway: an in-memory or
    /// persisted route, or a live client configuration pointing at the
    /// loopback endpoint. Exit decisions must consult the persisted facts and
    /// real client files, not only successfully restored in-memory routes.
    pub(crate) fn has_gateway_dependency(&self, local: &LocalState) -> bool {
        if self.has_active_routes() {
            return true;
        }
        if self
            .inner
            .state
            .lock()
            .map(|state| state.has_routes())
            .unwrap_or(true)
        {
            return true;
        }
        points_at_gateway_files(local)
    }

    /// Blocks new requests and waits for in-flight ones to finish. Returns
    /// `false` when long requests outlast the drain budget; the caller must
    /// then resume serving without changing anything.
    pub(crate) fn enter_maintenance_and_drain(&self) -> bool {
        self.inner.maintenance.store(true, Ordering::Release);
        let deadline = std::time::Instant::now() + PORT_CHANGE_DRAIN_TIMEOUT;
        while std::time::Instant::now() < deadline {
            if self.inner.inflight.load(Ordering::Acquire) == 0 {
                return true;
            }
            thread::sleep(std::time::Duration::from_millis(100));
        }
        self.inner.inflight.load(Ordering::Acquire) == 0
    }

    pub(crate) fn exit_maintenance(&self) {
        self.inner.maintenance.store(false, Ordering::Release);
    }
}

fn projection_warning(plan: &SwitchPlan, _projected: &SwitchPlan) -> String {
    let protocol = plan
        .profile
        .upstream_protocol
        .expect("validated custom protocol");
    let reason = if protocol == UpstreamProtocol::native_for(plan.profile.app) {
        "Codex 第三方请求统一经过本机网关并保留官方登录".to_string()
    } else {
        format!(
            "该供应商的上游协议是 {}，与 {} 原生协议（{}）不同",
            protocol.label(),
            plan.profile.app.label(),
            UpstreamProtocol::native_for(plan.profile.app).label()
        )
    };
    let mut warning = format!(
        "{reason}：切换后客户端配置中的服务地址会被改写为本机协议网关地址 {}（127.0.0.1 仅监听本机），请求由网关处理后以 {} 格式通过 HTTP(S)/SSE 携带原 API 密钥转发到所填服务地址，原地址与密钥只保存在本应用内。第三方请求需要本应用持续运行；退出后请重新打开本应用或切换到官方登录。",
        asb_core::redact::REDACTED, protocol.label(),
    );
    if plan.profile.responses_options.is_some_and(|options| {
        options.request_mode == asb_core::contracts::ResponsesRequestMode::Minimal
    }) {
        warning.push_str(" 最小模式省略 reasoning、service_tier、store、include、metadata 与客户端缓存扩展字段，保留输入上下文和工具调用；非空 previous_response_id 必须改为完整 input。");
    }
    if plan.profile.app == AppKind::Codex && protocol != UpstreamProtocol::Responses {
        warning.push_str(
            " Codex 的网页搜索会在此路由中关闭，因为它是该上游无法无损承载的 Responses 服务端工具；client_metadata、prompt_cache_key、reasoning.summary=auto 与 reasoning.encrypted_content 也不会转发到上游或出现在转换后的输出。需要这些 Responses 能力时，请使用 Responses 上游。",
        );
    }
    warning
}
