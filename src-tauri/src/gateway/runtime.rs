//! Listener runtime and Claude upstream health observations.

use super::*;

impl GatewayInner {
    /// The loopback base URL derived from the persisted port — the single
    /// address contract, read live so a port change takes effect everywhere.
    pub(crate) fn configured_base_url(&self) -> String {
        format!(
            "http://127.0.0.1:{}",
            self.state
                .lock()
                .map(|state| state.port)
                .unwrap_or(DEFAULT_GATEWAY_PORT)
        )
    }

    pub(crate) fn health_for(&self, route: &ActiveRoute) -> Arc<ProviderHealth> {
        if route.app == AppKind::Codex {
            let config = codex::policy::load(&self.state_root)
                .map(|(policy, _)| policy.traffic.health_config())
                .unwrap_or_default();
            return self.codex_health.health_for(route, config);
        }
        let config = failover::load(&self.state_root)
            .unwrap_or_default()
            .traffic
            .health_config();
        self.health_for_config(route, config)
    }

    pub(crate) fn health_for_config(
        &self,
        route: &ActiveRoute,
        config: ProviderHealthConfig,
    ) -> Arc<ProviderHealth> {
        let key = if route.app == AppKind::Claude {
            health_state::route_key(
                &route.profile_id,
                &route.fingerprint,
                &route.upstream_base_url,
            )
        } else {
            format!(
                "{}:{}:{}",
                route.profile_id, route.fingerprint, route.upstream_base_url
            )
        };
        if let Ok(health) = self.provider_health.read() {
            if let Some(existing) = health.get(&key).filter(|health| health.config() == config) {
                return Arc::clone(existing);
            }
        }
        let mut health = self
            .provider_health
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = health.get(&key).filter(|health| health.config() == config) {
            return Arc::clone(existing);
        }
        let provider_health = if route.app == AppKind::Claude {
            let snapshot = self.health_store.snapshot(&key);
            let store = Arc::clone(&self.health_store);
            let route_key = key.clone();
            let persistence: Arc<dyn Fn(provider_health::ProviderHealthSnapshot) + Send + Sync> =
                Arc::new(move |snapshot| {
                    if let Err(error) = store.save_snapshot(&route_key, &snapshot) {
                        log::warn!("无法持久化 Claude 供应商健康状态：{error}");
                    }
                });
            ProviderHealth::with_snapshot_and_persistence(config, snapshot, Some(persistence))
        } else {
            ProviderHealth::new(config)
        };
        let provider_health = Arc::new(provider_health);
        health.insert(key, Arc::clone(&provider_health));
        provider_health
    }

    pub(crate) fn candidates_for(&self, app: AppKind, active: &ActiveRoute) -> Vec<ActiveRoute> {
        self.candidate_routes
            .read()
            .ok()
            .and_then(|routes| routes.get(&app).cloned())
            .filter(|routes| !routes.is_empty())
            .unwrap_or_else(|| vec![active.clone()])
    }

    /// Records a successful Claude endpoint without touching the client
    /// configuration or changing the route identity. The shared write lock
    /// keeps this metadata-only update from racing a confirmed provider save.
    pub(crate) fn record_endpoint_success(&self, route: &ActiveRoute) {
        if route.app != AppKind::Claude {
            return;
        }
        let Ok(_guard) = self.endpoint_write_lock.lock() else {
            log::warn!("Claude 供应商端点使用记录锁不可用");
            return;
        };
        let store = crate::config_store::ConfigStore::new(self.state_root.clone());
        match store.mark_provider_endpoint_used(&route.profile_id, &route.upstream_base_url) {
            Ok(true) => self.prioritize_endpoint(route),
            Ok(false) => {}
            Err(error) => log::warn!("无法记录 Claude 供应商端点使用时间: {error}"),
        }
    }

    fn prioritize_endpoint(&self, route: &ActiveRoute) {
        let Ok(mut candidates) = self.candidate_routes.write() else {
            return;
        };
        let Some(routes) = candidates.get_mut(&route.app) else {
            return;
        };
        let Some(position) = routes.iter().position(|candidate| {
            candidate.profile_id == route.profile_id
                && candidate.upstream_base_url == route.upstream_base_url
        }) else {
            return;
        };
        let first_for_profile = routes
            .iter()
            .position(|candidate| candidate.profile_id == route.profile_id)
            .unwrap_or(position);
        if position == first_for_profile {
            return;
        }
        let candidate = routes.remove(position);
        routes.insert(first_for_profile, candidate);
    }
}
