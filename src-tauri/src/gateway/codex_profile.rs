use super::*;
use asb_core::contracts::{CodexProviderFile, CodexSubagentRoute};

/// Builds one Codex [`ActiveRoute`] from a validated provider file. A
/// subagent-routed profile also merges the referenced model into its
/// admission catalog so the wire id resolves locally.
pub(crate) fn codex_route_from_file(
    state_root: &std::path::Path,
    file: &asb_core::contracts::CodexProviderFile,
    identity: &str,
    replacement: Option<&CodexProviderFile>,
) -> Result<ActiveRoute, String> {
    let fingerprint = codex_route_fingerprint(file)?;
    let mut snapshot = asb_core::contracts::CodexRouteSnapshot::from_profile(
        &file.profile,
        fingerprint.clone(),
    )?;
    if let Some(entry) = subagent_catalog_entry(state_root, file, replacement)? {
        snapshot.catalog.push(entry);
    }
    let projection = file.client_projection().into_profile(AppKind::Codex);
    Ok(ActiveRoute {
        app: AppKind::Codex,
        profile_id: file.profile.id.clone(),
        client_token: route_token(identity, AppKind::Codex, &file.profile.id, &fingerprint),
        continuation_key: codex_continuation_key(identity, &file.profile.id, &fingerprint),
        fingerprint,
        upstream_base_url: super::routing::xai_pinned_upstream(&file.profile)
            .unwrap_or_else(|| file.profile.endpoint.0.clone()),
        connection: file.profile.connection.clone(),
        upstream_protocol: file.profile.upstream.protocol(),
        responses_options: projection.responses_options,
        max_output_tokens: file
            .profile
            .catalog
            .iter()
            .find(|entry| entry.id == file.profile.default_model)
            .map(|entry| entry.max_output_tokens),
        api_key: file.profile.api_key.clone(),
        authentication: file
            .profile
            .upstream
            .protocol()
            .resolve_authentication(file.profile.authentication),
        claude_fragment: Default::default(),
        claude_primary_model: None,
        claude_model_options: None,
        claude_account: None,
        codex_account: None,
        codex: Some(snapshot),
    })
}

/// Resolves the wire-id catalog entry a subagent route contributes: the
/// target profile's model, renamed to the route's wire id and labelled with
/// the target's name. The store is read live, so a deleted or edited target
/// fails loudly here instead of serving a stale entry.
pub(crate) fn subagent_catalog_entry(
    state_root: &std::path::Path,
    owner: &CodexProviderFile,
    replacement: Option<&CodexProviderFile>,
) -> Result<Option<asb_core::contracts::CodexCatalogEntry>, String> {
    let Some(route) = &owner.profile.subagent_route else {
        return Ok(None);
    };
    let store = crate::config_store::ConfigStore::new(state_root.to_path_buf());
    let file = if route.profile_id == owner.profile.id {
        owner.clone()
    } else if let Some(candidate) = replacement.filter(|file| file.profile.id == route.profile_id) {
        candidate.clone()
    } else {
        store.find_codex_provider_file(&route.profile_id).map_err(|_| {
            format!(
                "子代理路由引用的 Codex 供应商不存在：{}（模型 {}）",
                route.profile_id, route.model
            )
        })?
    };
    let entry = validate_subagent_target(route, &file)?;
    let mut merged = entry.clone();
    merged.id = route.wire_id();
    merged.display_name = Some(format!(
        "{} · {}",
        file.profile.name,
        entry.display_name.as_deref().unwrap_or(entry.id.as_str())
    ));
    Ok(Some(merged))
}

pub(crate) fn validate_subagent_target<'a>(
    route: &CodexSubagentRoute,
    file: &'a CodexProviderFile,
) -> Result<&'a asb_core::contracts::CodexCatalogEntry, String> {
    if file.profile.id != route.profile_id {
        return Err("子代理路由与目标供应商标识不一致".into());
    }
    if file.profile.connection.auth_binding.is_some() {
        return Err(format!(
            "子代理路由的目标供应商 {} 绑定了账号凭据，不能被其他路由转发",
            file.profile.name
        ));
    }
    file.profile
        .catalog
        .iter()
        .find(|entry| entry.id == route.model)
        .ok_or_else(|| {
            format!(
                "子代理路由的模型不在目标供应商目录中：{}（模型 {}）",
                file.profile.name, route.model
            )
        })
}
