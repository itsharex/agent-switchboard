//! Already-confirmed scene applications use the same complete preview as the UI.
use super::{
    execute_switch_core,
    plan::{build_plan, preview_projection},
};
use crate::commands::error::{blocking, state, CommandError};
use asb_switch::SwitchOutcome;
use tauri::{AppHandle, Manager};

pub(crate) async fn switch_provider_internal(
    app: AppHandle,
    profile_id: String,
) -> Result<SwitchOutcome, CommandError> {
    let preview = blocking({
        let app = app.clone();
        let profile_id = profile_id.clone();
        move || {
            let state = state(&app)?;
            let gateway = app
                .state::<crate::gateway::GatewayController>()
                .inner()
                .clone();
            let projection = build_plan(&state, &gateway, &profile_id)?;
            preview_projection(&state, &projection)
        }
    })
    .await?;
    execute_switch_core(
        app,
        profile_id,
        preview.content_hash,
        preview.rendered_hash,
        preview.auth_hash,
        preview.auth_existed,
        preview.auth_rendered_hash,
    )
    .await
}
