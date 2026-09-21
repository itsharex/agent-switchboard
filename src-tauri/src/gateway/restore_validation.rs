use super::*;

impl GatewayController {
    /// Rebuilds a derived catalog only from the exact provider revision that
    /// owns this saved configuration. This method never writes client files.
    pub(crate) fn restored_codex_catalog(
        &self,
        local: &LocalState,
        configuration: &str,
    ) -> Result<Option<CodexCatalogProjection>, String> {
        let catalog_pointer = configuration
            .parse::<toml_edit::DocumentMut>()
            .ok()
            .and_then(|document| {
                document
                    .get("model_catalog_json")
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
            });
        let is_gateway = routing::config_points_at_gateway(AppKind::Codex, configuration);
        if !is_gateway && catalog_pointer.is_none() {
            return Ok(None);
        }
        for record in local
            .configuration()
            .list_codex_providers()
            .map_err(|e| e.to_string())?
            .into_iter()
            .filter(|record| {
                is_gateway
                    || record.profile.route_mode == asb_core::contracts::CodexRouteMode::Direct
            })
        {
            let file = local
                .configuration()
                .find_codex_provider_file(&record.profile.id)
                .map_err(|error| error.to_string())?;
            let revision = codex_route_fingerprint(&file)?;
            if is_gateway {
                let route = self.route_for_codex_file(&file, None)?;
                if self.route_matches_config(&route, configuration)? {
                    return CodexCatalogProjection::from_file(
                        &self.inner.state_root,
                        &file,
                        &revision,
                        None,
                    )
                    .map(Some);
                }
            } else if catalog_pointer.as_deref()
                == Some(codex_catalog_file_name(&file.profile.id, &revision, &file.profile.catalog).as_str())
            {
                return CodexCatalogProjection::from_file(&self.inner.state_root, &file, &revision, None)
                    .map(Some);
            }
        }
        Err("恢复目标的 Codex 供应商或路由修订已变化，无法重建模型目录".into())
    }

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
            adapter::codex::validate_restore_configuration(configuration)
                .map_err(|error| error.to_string())?;
            let route = adapter::route_state(app, configuration);
            if route.base_url.is_none() {
                return Ok(configuration.to_string());
            }
            if !routing::config_points_at_gateway(app, configuration) {
                return Ok(configuration.to_string());
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
        let saved_origin = restored_origin(&saved_base)?;
        let routes = match app {
            AppKind::Codex => local
                .configuration()
                .list_codex_providers()
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|record| {
                    local
                        .configuration()
                        .find_codex_provider_file(&record.profile.id)
                        .map_err(|error| error.to_string())
                        .and_then(|file| self.route_for_codex_file(&file, None))
                })
                .collect::<Result<Vec<_>, _>>()?,
            AppKind::Claude => local
                .configuration()
                .list_providers()
                .map_err(|e| e.to_string())?
                .into_iter()
                .filter(|record| {
                    record.profile.app == AppKind::Claude
                        && record.profile.route_mode == RouteMode::Custom
                })
                .map(|record| self.route_for_profile(&record.profile))
                .collect::<Result<Vec<_>, _>>()?,
        };
        for route in routes {
            if route.client_endpoint(&saved_origin) == saved_base {
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
        }
        Err("恢复目标的供应商或连接凭据已变化，请重新应用供应商".to_string())
    }
}

fn restored_origin(saved_base: &str) -> Result<String, String> {
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
    Ok(format!(
        "http://127.0.0.1:{}",
        saved_url.port_or_known_default().unwrap_or(80)
    ))
}
