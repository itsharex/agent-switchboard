//! Codex providers IPC forwarding; business logic stays in the typed command owner.
use super::super::http::{argument, as_json, optional_argument, InvokeRequest};
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
        "cancel_codex_profile_save" => command!(commands::switching::cancel_codex_profile_save(
            app.clone(),
            argument(&request.args, "preparationId")?
        )),
        "list_codex_presets" => command!(commands::codex_management::list_codex_presets()),
        "prepare_codex_preset" => command!(commands::codex_management::prepare_codex_preset(
            argument(&request.args, "presetId")?,
            argument(&request.args, "apiKey")?
        )),
        "search_codex_profiles" => command!(commands::codex_management::search_codex_profiles(
            app.clone(),
            argument(&request.args, "query")?
        )),
        "duplicate_codex_profile" => {
            command!(commands::codex_management::duplicate_codex_profile(
                app.clone(),
                argument(&request.args, "profileId")?,
                argument(&request.args, "expectedFileHash")?,
                optional_argument(&request.args, "name")?
            ))
        }
        "list_codex_profiles" => command!(commands::list_codex_profiles(app.clone())),
        "create_codex_profile" => command!(commands::create_codex_profile(
            app.clone(),
            argument(&request.args, "draft")?,
        )),
        "delete_codex_profile" => command!(commands::delete_codex_profile(
            app.clone(),
            argument(&request.args, "profileId")?,
            argument(&request.args, "expectedFileHash")?,
        )),
        "reorder_codex_profiles" => command!(commands::reorder_codex_profiles(
            app.clone(),
            argument(&request.args, "orderedIds")?,
            argument(&request.args, "expectedFileHashes")?,
        )),
        "prepare_codex_profile_save" => {
            command!(commands::switching::prepare_codex_profile_save(
                app.clone(),
                argument(&request.args, "profileId")?,
                argument(&request.args, "draft")?,
                argument(&request.args, "expectedFileHash")?,
            ))
        }
        "commit_codex_profile_save" => {
            command!(commands::switching::commit_codex_profile_save(
                app.clone(),
                argument(&request.args, "preparationId")?,
                argument(&request.args, "confirmWrite")?,
            ))
        }
        "import_discovered_codex_profile" => {
            command!(commands::import_discovered_codex_profile(app.clone()))
        }
        "list_codex_endpoints" => command!(commands::codex_endpoints::list_codex_endpoints(
            app.clone(),
            argument(&request.args, "providerId")?
        )),
        "add_codex_endpoint" => command!(commands::codex_endpoints::add_codex_endpoint(
            app.clone(),
            argument(&request.args, "providerId")?,
            argument(&request.args, "url")?,
            argument(&request.args, "expectedFileHash")?,
            argument(&request.args, "confirmWrite")?
        )),
        "remove_codex_endpoint" => command!(commands::codex_endpoints::remove_codex_endpoint(
            app.clone(),
            argument(&request.args, "providerId")?,
            argument(&request.args, "url")?,
            argument(&request.args, "expectedFileHash")?,
            argument(&request.args, "confirmWrite")?
        )),
        "test_codex_endpoints" => command!(commands::codex_endpoints::test_codex_endpoints(
            argument(&request.args, "urls")?
        )),
        _ => Ok(None),
    }
}
