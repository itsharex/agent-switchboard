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
                next.active.remove(&app);
            }
            GatewayActivation::Routed(route) => {
                next.active.insert(
                    app,
                    PersistedRoute {
                        profile_id: route.profile_id.clone(),
                        fingerprint: route.fingerprint.clone(),
                    },
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
            let state_restore = write_state(&self.inner.state_path, &previous_state);
            if state_restore.is_ok() {
                *state = previous_state;
                if let Ok(mut routes) = self.inner.routes.write() {
                    match previous_route {
                        Some(route) => {
                            routes.insert(app, route);
                        }
                        None => {
                            routes.remove(&app);
                        }
                    }
                } else {
                    return Err("保存切换记录失败，且无法恢复本机协议网关路由".to_string());
                }
            } else {
                return Err("保存切换记录失败，且无法恢复本机协议网关状态".to_string());
            }
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn rehydrate(&self, local: &LocalState) {
        let persisted = match self.inner.state.lock() {
            Ok(state) => state.active.clone(),
            Err(_) => return,
        };
        let mut valid = BTreeMap::new();
        for (app, saved) in persisted {
            let Ok(profile) = local.configuration().find_provider(&saved.profile_id) else {
                log::warn!("本机协议网关无法恢复一个已删除的供应商路由");
                continue;
            };
            if profile.app != app
                || route_fingerprint(&profile).ok().as_deref() != Some(&saved.fingerprint)
            {
                log::warn!("本机协议网关拒绝恢复已变更的供应商路由");
                continue;
            }
            let Ok(route) = self.route_for_profile(&profile) else {
                log::warn!("本机协议网关拒绝恢复不再需要转换的供应商路由");
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
            let auth = if app == AppKind::Codex {
                match LocalState::codex_auth_path()
                    .ok()
                    .and_then(|path| fs::read_to_string(path).ok())
                {
                    Some(value) => Some(value),
                    None => {
                        log::warn!("本机协议网关拒绝恢复缺少 Codex 登录缓存的供应商路由");
                        continue;
                    }
                }
            } else {
                None
            };
            if !self
                .route_matches_config(&route, &text, auth.as_deref())
                .unwrap_or(false)
            {
                log::warn!("本机协议网关拒绝恢复未匹配当前客户端配置的供应商路由");
                continue;
            }
            valid.insert(app, route);
        }
        let expected: BTreeMap<AppKind, PersistedRoute> = valid
            .iter()
            .map(|(app, route)| {
                (
                    *app,
                    PersistedRoute {
                        profile_id: route.profile_id.clone(),
                        fingerprint: route.fingerprint.clone(),
                    },
                )
            })
            .collect();
        if let Ok(mut state) = self.inner.state.lock() {
            if state.active != expected {
                let mut next = state.clone();
                next.active = expected;
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

    pub(super) fn route_for_profile(
        &self,
        profile: &ProviderProfile,
    ) -> Result<ActiveRoute, String> {
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
            client_token: route_token(&identity, &fingerprint),
            fingerprint,
            upstream_base_url,
            upstream_protocol,
            max_output_tokens: profile.max_output_tokens.value(),
            api_key: profile.api_key.clone(),
        })
    }

    pub(super) fn route_matches_config(
        &self,
        route: &ActiveRoute,
        configuration: &str,
        codex_auth: Option<&str>,
    ) -> Result<bool, String> {
        adapter::matches_provider_identity(
            configuration,
            codex_auth,
            &self.client_identity_plan(route),
        )
        .map_err(|_| "无法验证本机协议网关客户端凭据".to_string())
    }

    pub(super) fn points_to_gateway(&self, app: AppKind, configuration: &str) -> bool {
        let route = adapter::route_state(app, configuration);
        let expected = match app {
            AppKind::Codex => format!("{}/v1", self.inner.base_url),
            AppKind::Claude => self.inner.base_url.clone(),
        };
        route.base_url.as_deref() == Some(expected.as_str())
    }

    pub(super) fn client_identity_plan(&self, route: &ActiveRoute) -> SwitchPlan {
        SwitchPlan::through_gateway(
            ProviderProfile {
                id: route.profile_id.clone(),
                app: route.app,
                route_mode: RouteMode::Custom,
                name: "本机协议网关".to_string(),
                model: None,
                base_url: Some(match route.app {
                    AppKind::Codex => format!("{}/v1", self.inner.base_url),
                    AppKind::Claude => self.inner.base_url.clone(),
                }),
                api_key: route.client_token.clone(),
                upstream_protocol: Some(match route.app {
                    AppKind::Codex => UpstreamProtocol::Responses,
                    AppKind::Claude => UpstreamProtocol::AnthropicMessages,
                }),
                max_output_tokens: None.into(),
                model_options: None,
                notes: None,
                website_url: None,
                usage_query: None,
                official_quota_refresh_interval_minutes: None,
            },
            asb_core::ownership::default_common_settings(route.app),
        )
    }
}
