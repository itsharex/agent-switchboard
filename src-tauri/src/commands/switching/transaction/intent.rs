//! Construction of the application half of a durable configuration write.
use super::*;
use asb_core::contracts::{ClientSettingsSnapshot, SettingsValues};

pub(in crate::commands::switching) fn begin(
    state: &LocalState,
    gateway: &GatewayController,
    app: AppKind,
    profile_id: Option<&str>,
    after_hash: &str,
    after_existed: bool,
    catalog: Option<CatalogArtifact>,
) -> Result<(), CommandError> {
    begin_with_codex_backfill_and_auth(
        state, gateway, app, profile_id, after_hash, after_existed, catalog, None, None,
    )
}

pub(in crate::commands::switching) fn begin_with_codex_backfill_and_auth(
    state: &LocalState,
    gateway: &GatewayController,
    app: AppKind,
    profile_id: Option<&str>,
    after_hash: &str,
    after_existed: bool,
    catalog: Option<CatalogArtifact>,
    codex_backfill: Option<&PreparedCodexBackfill>,
    auth: Option<AuthIntent>,
) -> Result<(), CommandError> {
    let intent = prepare(
        state, gateway, app, profile_id, after_hash, after_existed, catalog, codex_backfill, auth,
    )?;
    journal::save(state, &intent)
}

pub(in crate::commands) fn begin_client_configuration(
    state: &LocalState,
    gateway: &GatewayController,
    app: AppKind,
    after_hash: &str,
    after_existed: bool,
    settings: Option<(&ClientSettingsSnapshot, &SettingsValues)>,
) -> Result<(), CommandError> {
    let mut intent = prepare(state, gateway, app, None, after_hash, after_existed, None, None, None)?;
    intent.operation = WriteOperation::Projection;
    intent.client_settings = settings
        .map(|(before, after)| settings::ClientSettingsIntent::new(app, before, after))
        .transpose().map_err(error)?;
    journal::save(state, &intent)
}

pub(in crate::commands::switching) fn begin_restore(
    state: &LocalState,
    gateway: &GatewayController,
    app: AppKind,
    after_hash: &str,
    after_existed: bool,
    catalog: Option<CatalogArtifact>,
    auth: Option<AuthIntent>,
    before: &ClientSettingsSnapshot,
    after: &SettingsValues,
) -> Result<(), CommandError> {
    let mut intent = prepare(state, gateway, app, None, after_hash, after_existed, catalog, None, auth)?;
    intent.client_settings = Some(settings::ClientSettingsIntent::new(app, before, after).map_err(error)?);
    journal::save(state, &intent)
}

fn prepare(
    state: &LocalState,
    gateway: &GatewayController,
    app: AppKind,
    profile_id: Option<&str>,
    after_hash: &str,
    after_existed: bool,
    catalog: Option<CatalogArtifact>,
    codex_backfill: Option<&PreparedCodexBackfill>,
    auth: Option<AuthIntent>,
) -> Result<SwitchIntent, CommandError> {
    if load(state).map_err(error)?.is_some() {
        return Err(CommandError::keyed(
            "config-recovery-required",
            "errors.sw.pendingTransactionExists",
            "存在未完成配置事务，请先恢复",
        ));
    }
    let target = state.target(app).map_err(error)?;
    let (before, before_existed) = read_target(&target, app).map_err(error)?;
    if let Some(auth) = &auth {
        if app != AppKind::Codex
            || !auth_snapshot_matches(&target, &auth.before_hash, auth.before_existed).map_err(error)?
        {
            return Err(CommandError::keyed(
                "config-recovery-required",
                "errors.sw.codexAuthChangedAfterPreview",
                "Codex 认证文件在预览后已发生变化，请重新查看差异",
            ));
        }
    }
    validate_catalog_target(&target, catalog.as_ref()).map_err(error)?;
    let profile_hash = profile_id
        .map(|id| profile_revision(state, app, id)).transpose().map_err(error)?;
    Ok(SwitchIntent {
        version: 2,
        app,
        target: target.to_string_lossy().into_owned(),
        profile_id: profile_id.map(str::to_string),
        profile_hash,
        before_hash: asb_switch::sha256_hex(&before),
        before_existed,
        after_hash: after_hash.into(),
        after_existed,
        previous_route: gateway.snapshot_activation(app).map_err(error)?,
        previous_write: state.configuration().latest_config_write(app)
            .map_err(|e| error(e.to_string()))?,
        catalog,
        codex_backfill: codex_backfill.map(|backfill| CodexBackfillIntent {
            profile_id: backfill.profile_id.clone(),
            before_hash: backfill.before_hash.clone(),
            after_hash: backfill.after_hash.clone(),
            before: backfill.before.clone(),
        }),
        auth,
        operation: if profile_id.is_some() { WriteOperation::Projection } else { WriteOperation::Restore },
        client_settings: None,
    })
}
