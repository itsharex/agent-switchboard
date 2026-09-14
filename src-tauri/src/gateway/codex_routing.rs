//! Codex catalog projections, separate from Claude provider routing.

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
        let (policy, _) = codex::policy::load(&self.inner.state_root)?;
        self.project_codex_with_policy(file, client_settings, &policy)
    }

    pub(crate) fn project_codex_with_policy(
        &self,
        file: &asb_core::contracts::CodexProviderFile,
        client_settings: asb_core::contracts::SettingsValues,
        policy: &codex::policy::CodexGatewayPolicy,
    ) -> Result<GatewayProjection, String> {
        policy.validate()?;
        let preserve = crate::codex_auth::policy::load(&self.inner.state_root)?.policy.preserve_official_login;
        file.validate()?;
        let profile = file.client_projection().into_profile(AppKind::Codex);
        let catalog = CodexCatalogProjection::from_file(file, &codex_route_fingerprint(file)?)?;
        if file.profile.route_mode == asb_core::contracts::CodexRouteMode::Direct
            && !policy.takeover
        {
            let plan = SwitchPlan::direct(profile, client_settings)
                .with_codex_model_catalog(catalog.file_name.clone())
                .with_codex_preserve_official_login(preserve);
            asb_core::validate_plan(&plan.profile, &plan.client_settings)
                .map_err(|error| error.to_string())?;
            return Ok(GatewayProjection {
                plan,
                activation: GatewayActivation::Direct { app: AppKind::Codex },
                warning: Some(
                    "Codex Responses 将直连供应商；API 密钥仅写入 Codex auth.json，不写入 config.toml，也不会启动本机网关路由"
                        .to_string(),
                ),
                codex_catalog: Some(catalog),
                candidate_routes: Vec::new(),
            });
        }
        if self.blocked_recovery().is_some() {
            return Err("存在未完成的端口修改恢复，无法切换 Codex 供应商".to_string());
        }
        let base_url = self
            .listening_base_url()
            .ok_or_else(|| "本机协议网关当前未在监听，无法写入 Codex 第三方配置".to_string())?;
        let route = self.route_for_codex_file(file)?;
        let catalog = CodexCatalogProjection::from_file(file, &route.fingerprint)?;
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
            candidate_routes: self.codex_candidate_routes(file, policy)?,
            activation: GatewayActivation::Routed(route),
            warning: Some(warning),
            codex_catalog: Some(catalog),
        })
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
            .with_codex_model_catalog(codex_catalog_file_name(&file.profile.id, &revision)),
        ))
    }
}
