//! Route resolution, activation commit, and restore reconciliation.

use super::*;

impl GatewayController {
    pub(super) fn commit_activation<F>(
        &self,
        activation: &GatewayActivation,
        persist: F,
    ) -> Result<(), String>
    where
        F: FnOnce() -> Result<(), String>,
    {
        let _activation = self
            .inner
            .activation_lock
            .lock()
            .map_err(|_| "本机协议网关切换锁不可用".to_string())?;
        let app = activation.app();
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| "本机协议网关状态锁不可用".to_string())?;
        let mut next = state.clone();
        match activation {
            GatewayActivation::Direct { .. } => {
                next.set_route(app, None);
            }
            GatewayActivation::Routed(route) => {
                next.set_route(
                    app,
                    Some(PersistedRoute {
                        profile_id: route.profile_id.clone(),
                        revision: route.fingerprint.clone(),
                    }),
                );
            }
        }
        let previous_state = state.clone();
        let previous_route = self
            .inner
            .routes
            .read()
            .map_err(|_| "本机协议网关路由锁不可用".to_string())?
            .get(&app)
            .cloned();
        write_state(&self.inner.state_path, &next)?;
        *state = next;
        let mut routes = self
            .inner
            .routes
            .write()
            .map_err(|_| "本机协议网关路由锁不可用".to_string())?;
        match activation {
            GatewayActivation::Direct { .. } => {
                routes.remove(&app);
            }
            GatewayActivation::Routed(route) => {
                routes.insert(app, route.clone());
            }
        }
        drop(routes);
        if let Err(error) = persist() {
            self.restore_previous_activation(app, &mut state, previous_state, previous_route)?;
            return Err(error);
        }
        Ok(())
    }

    fn restore_previous_activation(
        &self,
        app: AppKind,
        state: &mut GatewayStateFile,
        previous_state: GatewayStateFile,
        previous_route: Option<ActiveRoute>,
    ) -> Result<(), String> {
        write_state(&self.inner.state_path, &previous_state)
            .map_err(|error| format!("保存切换记录失败，且无法恢复网关状态：{error}"))?;
        *state = previous_state;
        let mut routes = self
            .inner
            .routes
            .write()
            .map_err(|_| "保存切换记录失败，且无法恢复网关路由".to_string())?;
        match previous_route {
            Some(route) => {
                routes.insert(app, route);
            }
            None => {
                routes.remove(&app);
            }
        }
        Ok(())
    }

    pub(super) fn rehydrate(&self, local: &LocalState) {
        let Some(valid) = self.recoverable_routes(local) else {
            return;
        };
        if let Ok(mut state) = self.inner.state.lock() {
            let codex = valid.get(&AppKind::Codex).map(persisted_route);
            let claude = valid.get(&AppKind::Claude).map(persisted_route);
            if state.codex_route != codex || state.claude_route != claude {
                let mut next = state.clone();
                next.codex_route = codex;
                next.claude_route = claude;
                if write_state(&self.inner.state_path, &next).is_ok() {
                    *state = next;
                } else {
                    // Refuse to publish a route whose persistence repair
                    // failed. The local client configuration will never be
                    // silently rebound to an unrecorded provider.
                    return;
                }
            }
        } else {
            return;
        }
        if let Ok(mut routes) = self.inner.routes.write() {
            *routes = valid;
        }
    }

    fn recoverable_routes(&self, local: &LocalState) -> Option<BTreeMap<AppKind, ActiveRoute>> {
        let persisted = self.inner.state.lock().ok()?.clone();
        let mut valid = BTreeMap::new();
        for app in [AppKind::Codex, AppKind::Claude] {
            let Some(saved) = persisted.route(app).cloned() else {
                continue;
            };
            let route = match app {
                AppKind::Codex => local
                    .configuration()
                    .find_codex_provider_file(&saved.profile_id)
                    .map_err(|error| error.to_string())
                    .and_then(|file| {
                        (codex_route_fingerprint(&file)? == saved.revision)
                            .then_some(file)
                            .ok_or_else(|| "Codex 路由已改变".to_string())
                    })
                    .and_then(|file| self.route_for_codex_file(&file)),
                AppKind::Claude => local
                    .configuration()
                    .find_provider(&saved.profile_id)
                    .and_then(|profile| {
                        (profile.app == app && route_fingerprint(&profile)? == saved.revision)
                            .then_some(profile)
                            .ok_or_else(|| "Claude 路由已改变".to_string())
                    })
                    .and_then(|profile| self.route_for_profile(&profile)),
            };
            let Ok(route) = route else {
                log::warn!("本机协议网关拒绝恢复已删除或已变更的供应商路由");
                continue;
            };
            let Ok(target) = local.target(app) else {
                log::warn!("本机协议网关无法读取客户端配置路径");
                continue;
            };
            let Ok(text) = fs::read_to_string(target) else {
                log::warn!("本机协议网关拒绝恢复未指向本机端口的供应商路由");
                continue;
            };
            if !self.route_matches_config(&route, &text).unwrap_or(false) {
                log::warn!("本机协议网关拒绝恢复未匹配当前客户端配置的供应商路由");
                continue;
            }
            valid.insert(app, route);
        }
        Some(valid)
    }

    /// A client that carries this gateway's token but does not exactly match
    /// the currently recoverable route needs an explicit re-apply. This is
    /// derived from current files rather than a boot-only flag, so the repair
    /// state survives an application restart.
    pub(super) fn has_unreconciled_gateway_files(&self, local: &LocalState) -> bool {
        for app in [AppKind::Codex, AppKind::Claude] {
            let Ok(target) = local.target(app) else {
                continue;
            };
            let Ok(configuration) = fs::read_to_string(target) else {
                continue;
            };
            if !config_points_at_gateway(app, &configuration) {
                continue;
            }
            let route = match self.inner.routes.read() {
                Ok(routes) => routes.get(&app).cloned(),
                Err(_) => return true,
            };
            if !route.is_some_and(|route| {
                self.route_matches_config(&route, &configuration)
                    .unwrap_or(false)
            }) {
                return true;
            }
        }
        false
    }

    pub(super) fn route_for_profile(
        &self,
        profile: &ProviderProfile,
    ) -> Result<ActiveRoute, String> {
        if profile.app == AppKind::Codex {
            return Err("Codex 必须从专用档案创建网关路由".to_string());
        }
        if profile.route_mode != RouteMode::Custom {
            return Err("官方登录不应创建本机协议网关路由".to_string());
        }
        if is_direct(profile) {
            return Err("直连供应商不应创建本机协议网关路由".to_string());
        }
        let upstream_protocol = profile
            .upstream_protocol
            .ok_or_else(|| "供应商缺少上游 API 格式".to_string())?;
        let upstream_base_url = profile
            .base_url
            .clone()
            .ok_or_else(|| "供应商缺少服务地址".to_string())?;
        let fingerprint = route_fingerprint(profile)?;
        let identity = self
            .inner
            .state
            .lock()
            .map_err(|_| "本机协议网关状态锁不可用".to_string())?
            .identity
            .clone();
        Ok(ActiveRoute {
            app: profile.app,
            profile_id: profile.id.clone(),
            client_token: route_token(&identity, profile.app, &profile.id, &fingerprint),
            continuation_key: continuation_key(&identity, profile),
            fingerprint,
            upstream_base_url,
            upstream_protocol,
            responses_options: profile.responses_options.clone(),
            max_output_tokens: profile.max_output_tokens.value(),
            api_key: profile.api_key.clone(),
            codex: None,
        })
    }

    pub(super) fn route_for_codex_file(
        &self,
        file: &asb_core::contracts::CodexProviderFile,
    ) -> Result<ActiveRoute, String> {
        let fingerprint = codex_route_fingerprint(file)?;
        let snapshot = asb_core::contracts::CodexRouteSnapshot::from_profile(
            &file.profile,
            fingerprint.clone(),
        )?;
        let projection = file.client_projection().into_profile(AppKind::Codex);
        let identity = self
            .inner
            .state
            .lock()
            .map_err(|_| "本机协议网关状态锁不可用".to_string())?
            .identity
            .clone();
        Ok(ActiveRoute {
            app: AppKind::Codex,
            profile_id: file.profile.id.clone(),
            client_token: route_token(&identity, AppKind::Codex, &file.profile.id, &fingerprint),
            continuation_key: codex_continuation_key(&identity, &file.profile.id, &fingerprint),
            fingerprint,
            upstream_base_url: file.profile.endpoint.0.clone(),
            upstream_protocol: file.profile.upstream.protocol(),
            responses_options: projection.responses_options,
            max_output_tokens: file
                .profile
                .catalog
                .iter()
                .find(|entry| entry.id == file.profile.default_model)
                .map(|entry| entry.max_output_tokens),
            api_key: file.profile.api_key.clone(),
            codex: Some(snapshot),
        })
    }

    pub(super) fn route_matches_config(
        &self,
        route: &ActiveRoute,
        configuration: &str,
    ) -> Result<bool, String> {
        adapter::matches_provider_identity(configuration, &self.client_identity_plan(route))
            .map_err(|_| "无法验证本机协议网关客户端凭据".to_string())
    }

    pub(super) fn client_identity_plan(&self, route: &ActiveRoute) -> SwitchPlan {
        let base_url = self.configured_base_url();
        SwitchPlan::through_gateway(
            ProviderProfile {
                id: route.profile_id.clone(),
                parameters: asb_core::ownership::default_provider_parameters(route.app),
                app: route.app,
                route_mode: RouteMode::Custom,
                name: "本机协议网关".to_string(),
                model: None,
                base_url: Some(route.upstream_base_url.clone()),
                api_key: route.api_key.clone(),
                upstream_protocol: Some(route.upstream_protocol),
                responses_options: route.responses_options,
                max_output_tokens: None.into(),
                model_options: None,
                notes: None,
                website_url: None,
                usage_query: None,
                official_quota_refresh_interval_minutes: None,
            },
            asb_core::ownership::default_client_settings(route.app),
            route.client_endpoint(&base_url),
            route.client_token.clone(),
        )
    }
}

fn persisted_route(route: &ActiveRoute) -> PersistedRoute {
    PersistedRoute {
        profile_id: route.profile_id.clone(),
        revision: route.fingerprint.clone(),
    }
}

/// The capability-token prefix this application writes into client
/// configurations. Combined with a loopback endpoint it identifies a client
/// that depends on this gateway without needing the listener to be up.
pub(super) const CLIENT_TOKEN_PREFIX: &str = "asb_local_";

/// Coarse, listener-independent ownership check: the client configuration
/// points at a loopback gateway endpoint carrying this application's
/// capability token. Used for repair detection, exit decisions, and restore
/// rejection — never to *authorize* a rewrite, which always requires the
/// exact identity match.
pub(super) fn config_points_at_gateway(app: AppKind, configuration: &str) -> bool {
    if adapter::validate_syntax(app, configuration).is_err() {
        return false;
    }
    let Some(base_url) = adapter::route_state(app, configuration).base_url else {
        return false;
    };
    if !base_url.starts_with("http://127.0.0.1:") {
        return false;
    }
    match app {
        AppKind::Codex => asb_core::adapter::codex::is_gateway_base_url(&base_url),
        AppKind::Claude => configuration.contains(CLIENT_TOKEN_PREFIX),
    }
}
