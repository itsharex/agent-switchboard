//! Codex gateway IPC forwarding; business logic stays in the typed command owner.
use super::super::http::{argument, as_json, InvokeRequest};
use crate::commands::{self, error::CommandError};
use serde_json::Value;
use tauri::AppHandle;

macro_rules! command {
    ($future:expr) => {
        as_json($future.await).map(Some)
    };
}

pub(super) async fn dispatch(
    app: &AppHandle,
    request: &InvokeRequest,
) -> Result<Option<Value>, CommandError> {
    match request.command.as_str() {
        "get_outbound_proxy" => command!(commands::outbound_proxy::get_outbound_proxy(app.clone())),
        "set_outbound_proxy" => command!(commands::outbound_proxy::set_outbound_proxy(
            app.clone(),
            argument(&request.args, "settings")?,
            argument(&request.args, "expectedRevision")?,
            argument(&request.args, "confirmWrite")?,
        )),
        "test_outbound_proxy" => command!(commands::outbound_proxy::test_outbound_proxy(argument(
            &request.args,
            "url"
        )?,)),
        "scan_local_outbound_proxies" => command!(
            commands::outbound_proxy::scan_local_outbound_proxies(app.clone())
        ),
        "get_codex_gateway_policy" => command!(
            commands::switching::codex_policy::get_codex_gateway_policy(app.clone())
        ),
        "prepare_codex_gateway_policy" => command!(
            commands::switching::codex_policy::prepare_codex_gateway_policy(
                app.clone(),
                argument(&request.args, "profileId")?,
                argument(&request.args, "policy")?
            )
        ),
        "commit_codex_gateway_policy" => command!(
            commands::switching::codex_policy::commit_codex_gateway_policy(
                app.clone(),
                argument(&request.args, "preparationId")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        "cancel_codex_gateway_policy" => command!(
            commands::switching::codex_policy::cancel_codex_gateway_policy(
                app.clone(),
                argument(&request.args, "preparationId")?
            )
        ),
        "recover_codex_gateway_policy" => {
            command!(commands::switching::codex_policy::recover_codex_gateway_policy(app.clone()))
        }
        "discard_codex_gateway_policy" => command!(
            commands::switching::codex_policy::discard_codex_gateway_policy(
                app.clone(),
                argument(&request.args, "expectedConfigHash")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        "reset_codex_provider_health" => command!(
            commands::switching::codex_policy::reset_codex_provider_health(
                app.clone(),
                argument(&request.args, "profileId")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        "scan_codex_failover_source" => command!(
            commands::switching::codex_policy::scan_codex_failover_source(
                app.clone(),
                argument(&request.args, "sourcePath")?
            )
        ),
        "scan_codex_env_conflicts" => command!(commands::codex_env::scan_codex_env_conflicts()),
        "remove_codex_env_conflicts" => command!(commands::codex_env::remove_codex_env_conflicts(
            app.clone(),
            argument(&request.args, "selections")?,
            argument(&request.args, "expectedRevision")?,
            argument(&request.args, "confirmWrite")?
        )),
        "list_codex_env_backups" => {
            command!(commands::codex_env::list_codex_env_backups(app.clone()))
        }
        "restore_codex_env_backup" => command!(commands::codex_env::restore_codex_env_backup(
            app.clone(),
            argument(&request.args, "fileName")?,
            argument(&request.args, "confirmWrite")?
        )),
        _ => Ok(None),
    }
}
