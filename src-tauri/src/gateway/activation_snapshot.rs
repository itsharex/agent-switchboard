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
            route: state.active.get(&app).cloned(),
        })
    }

    pub(crate) fn restore_activation_snapshot(
        &self,
        local: &LocalState,
        snapshot: &GatewayActivationSnapshot,
    ) -> Result<(), String> {
        let activation = match &snapshot.route {
            None => GatewayActivation::Direct { app: snapshot.app },
            Some(saved) => {
                let profile = local
                    .configuration()
                    .find_provider(&saved.profile_id)
                    .map_err(|_| "恢复所需的供应商档案已不存在".to_string())?;
                if profile.app != snapshot.app || route_fingerprint(&profile)? != saved.fingerprint
                {
                    return Err("恢复所需的供应商连接已改变，保留恢复记录等待处理".to_string());
                }
                GatewayActivation::Routed(self.route_for_profile(&profile)?)
            }
        };
        self.commit_activation(&activation, || Ok(()))
    }
}
