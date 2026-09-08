use super::*;

impl GatewayController {
    pub(crate) fn validate_restored(
        &self,
        local: &LocalState,
        app: AppKind,
        configuration: &str,
    ) -> Result<(), String> {
        self.prepare_restored(local, app, configuration).map(|_| ())
    }

    /// Validate the saved route capability before rebinding only its loopback
    /// address. The backup remains immutable and other configuration is retained.
    pub(crate) fn prepare_restored(
        &self,
        local: &LocalState,
        app: AppKind,
        configuration: &str,
    ) -> Result<String, String> {
        adapter::validate_syntax(app, configuration).map_err(|error| error.to_string())?;
        if app == AppKind::Codex {
            validate_codex_provider(configuration)?;
            let route = adapter::route_state(app, configuration);
            if route.base_url.is_none() {
                return Ok(configuration.to_string());
            }
            if !routing::config_points_at_gateway(app, configuration) {
                return Err("恢复目标不是当前 Codex 网关路由，已拒绝恢复第三方直连配置".to_string());
            }
        } else if !routing::config_points_at_gateway(app, configuration) {
            return Ok(configuration.to_string());
        }
        if self.listening().is_none() || self.blocked_recovery().is_some() {
            return Err("本机网关尚未恢复监听或存在未完成的端口恢复，无法恢复第三方路由".into());
        }
        let saved_base = adapter::route_state(app, configuration)
            .base_url
            .ok_or_else(|| "恢复目标缺少本机网关地址".to_string())?;
        let saved_url =
            reqwest::Url::parse(&saved_base).map_err(|_| "恢复目标网关地址无效".to_string())?;
        if saved_url.scheme() != "http"
            || saved_url.host_str() != Some("127.0.0.1")
            || !saved_url.username().is_empty()
            || saved_url.password().is_some()
            || saved_url.query().is_some()
            || saved_url.fragment().is_some()
        {
            return Err("恢复目标不是有效的本机网关地址".into());
        }
        let saved_origin = format!(
            "http://127.0.0.1:{}",
            saved_url.port_or_known_default().unwrap_or(80)
        );
        for record in local
            .configuration()
            .list_providers()
            .map_err(|e| e.to_string())?
        {
            if record.profile.app != app || !record.profile.requires_gateway() {
                continue;
            }
            let route = self.route_for_profile(&record.profile)?;
            if route.client_endpoint(&saved_origin) != saved_base {
                continue;
            }
            let candidate = adapter::render_gateway_base_url(
                app,
                configuration,
                &route.client_endpoint(&self.configured_base_url()),
            )
            .map_err(|error| error.to_string())?;
            if self.route_matches_config(&route, &candidate)? {
                return Ok(candidate);
            }
        }
        Err("恢复目标的供应商或连接凭据已变化，请重新应用供应商".to_string())
    }
}

fn validate_codex_provider(configuration: &str) -> Result<(), String> {
    let document = configuration
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "恢复目标配置无法解析".to_string())?;
    let provider = document
        .get("model_provider")
        .map(|item| item.as_str())
        .unwrap_or(Some("openai"));
    if provider != Some("openai")
        || document
            .get("model_providers")
            .and_then(|providers| providers.get("openai"))
            .is_some()
    {
        return Err("该备份使用已停用的 Codex provider 契约，请重新应用供应商".to_string());
    }
    Ok(())
}
