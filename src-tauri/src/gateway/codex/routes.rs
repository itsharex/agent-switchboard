use super::policy::CodexGatewayPolicy;
use crate::gateway::{ActiveRoute, GatewayController};
use asb_core::contracts::{AppKind, CodexProviderFile};

impl GatewayController {
    pub(crate) fn codex_candidate_routes(
        &self,
        file: &CodexProviderFile,
        policy: &CodexGatewayPolicy,
    ) -> Result<Vec<ActiveRoute>, String> {
        policy.validate()?;
        let store = crate::config_store::ConfigStore::new(self.inner.state_root.clone());
        let mut routes = Vec::new();
        for id in policy.candidate_ids(&file.profile.id) {
            let candidate = if id == file.profile.id {
                file.clone()
            } else {
                store
                    .find_codex_provider_file(&id)
                    .map_err(|_| format!("Codex 队列供应商不存在：{id}"))?
            };
            if policy.enabled && candidate.profile.connection.auth_binding.is_some() {
                return Err("托管账号不能参与 Codex 自动故障转移".into());
            }
            let route = self.route_for_codex_file(&candidate)?;
            for endpoint in route
                .connection
                .endpoint_candidates(&route.upstream_base_url)
            {
                let mut variant = route.clone();
                variant.upstream_base_url = endpoint;
                routes.push(variant);
            }
        }
        Ok(routes)
    }

    pub(crate) fn rehydrate_codex_candidates(
        &self,
        profile_id: &str,
    ) -> Result<Vec<ActiveRoute>, String> {
        let store = crate::config_store::ConfigStore::new(self.inner.state_root.clone());
        let file = store
            .find_codex_provider_file(profile_id)
            .map_err(|error| error.to_string())?;
        let (policy, _) = super::policy::load(&self.inner.state_root)?;
        self.codex_candidate_routes(&file, &policy)
    }

    pub(crate) fn reset_codex_provider_health(&self, profile_id: &str) -> Result<(), String> {
        let store = crate::config_store::ConfigStore::new(self.inner.state_root.clone());
        let file = store
            .find_codex_provider_file(profile_id)
            .map_err(|error| error.to_string())?;
        let route = self.route_for_codex_file(&file)?;
        for endpoint in route
            .connection
            .endpoint_candidates(&route.upstream_base_url)
        {
            let mut variant = route.clone();
            variant.upstream_base_url = endpoint;
            self.inner.codex_health.reset(&variant)?;
        }
        Ok(())
    }

    pub(crate) fn codex_health_warning(&self) -> Option<String> {
        self.inner.codex_health.warning()
    }
}

impl crate::gateway::GatewayProjection {
    pub(crate) fn activation_snapshot(&self) -> crate::gateway::GatewayActivationSnapshot {
        use crate::gateway::GatewayActivation;
        let route = match &self.activation {
            GatewayActivation::Direct { .. } => None,
            GatewayActivation::Routed(route) => Some(crate::gateway::state::PersistedRoute {
                profile_id: route.profile_id.clone(),
                revision: route.fingerprint.clone(),
            }),
        };
        crate::gateway::GatewayActivationSnapshot {
            app: self.plan.app(),
            route,
        }
    }
    pub(crate) fn codex_candidate_revision(&self) -> String {
        let mut revisions = self
            .candidate_routes
            .iter()
            .map(|route| {
                (
                    &route.profile_id,
                    &route.fingerprint,
                    &route.upstream_base_url,
                )
            })
            .collect::<Vec<_>>();
        revisions.sort();
        asb_switch::sha256_hex(
            &serde_json::to_string(&revisions).expect("route identity serializes"),
        )
    }
}

impl GatewayController {
    pub(crate) fn clear_codex_activation(&self) -> Result<(), String> {
        self.commit_activation(
            &crate::gateway::GatewayActivation::Direct {
                app: AppKind::Codex,
            },
            &[],
            || Ok(()),
        )
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CodexEndpointHealth {
    endpoint: String,
    circuit_state: crate::gateway::ProviderHealthState,
    consecutive_failures: u32,
}
impl GatewayController {
    pub(crate) fn codex_endpoint_health(
        &self,
        file: &CodexProviderFile,
        policy: &CodexGatewayPolicy,
    ) -> Result<Vec<CodexEndpointHealth>, String> {
        let route = self.route_for_codex_file(file)?;
        Ok(route
            .connection
            .endpoint_candidates(&route.upstream_base_url)
            .into_iter()
            .map(|endpoint| {
                let mut candidate = route.clone();
                candidate.upstream_base_url = endpoint.clone();
                let health = self
                    .inner
                    .codex_health
                    .health_for(&candidate, policy.traffic.health_config());
                CodexEndpointHealth {
                    endpoint,
                    circuit_state: health.state(),
                    consecutive_failures: health.consecutive_failures(),
                }
            })
            .collect())
    }
}
