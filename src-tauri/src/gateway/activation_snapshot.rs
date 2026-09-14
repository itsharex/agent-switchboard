use super::*;

/// Recovery intent references a profile revision without copying credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GatewayActivationSnapshot {
    pub(crate) app: AppKind,
    pub(crate) route: Option<PersistedRoute>,
}

impl GatewayController {
    pub(crate) fn snapshot_activation(
        &self,
        app: AppKind,
    ) -> Result<GatewayActivationSnapshot, String> {
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| "网关状态锁不可用".to_string())?;
        Ok(GatewayActivationSnapshot {
            app,
            route: state.route(app).cloned(),
        })
    }

    pub(crate) fn restore_activation_snapshot(
        &self,
        local: &LocalState,
        snapshot: &GatewayActivationSnapshot,
    ) -> Result<(), String> {
        let mut candidate_routes = Vec::new();
        let activation = match &snapshot.route {
            None => GatewayActivation::Direct { app: snapshot.app },
            Some(saved) => {
                let route = match snapshot.app {
                    AppKind::Codex => {
                        let file = local
                            .configuration()
                            .find_codex_provider_file(&saved.profile_id)
                            .map_err(|_| "恢复所需的 Codex 供应商档案已不存在".to_string())?;
                        if codex_route_fingerprint(&file)? != saved.revision {
                            return Err("恢复所需的 Codex 供应商连接已改变，保留恢复记录等待处理"
                                .to_string());
                        }
                        let (policy, _) = codex::policy::load(local.root())?;
                        candidate_routes = self.codex_candidate_routes(&file, &policy)?;
                        self.route_for_codex_file(&file)?
                    }
                    AppKind::Claude => {
                        let profile = local
                            .configuration()
                            .find_provider(&saved.profile_id)
                            .map_err(|_| "恢复所需的供应商档案已不存在".to_string())?;
                        if profile.app != AppKind::Claude
                            || route_fingerprint(&profile)? != saved.revision
                        {
                            return Err(
                                "恢复所需的供应商连接已改变，保留恢复记录等待处理".to_string()
                            );
                        }
                        let route = self.route_for_profile(&profile)?;
                        candidate_routes = self
                            .claude_candidate_routes(local, &route.profile_id)
                            .unwrap_or_else(|_| vec![route.clone()]);
                        route
                    }
                };
                GatewayActivation::Routed(route)
            }
        };
        self.commit_activation(&activation, &candidate_routes, || Ok(()))
    }
}
