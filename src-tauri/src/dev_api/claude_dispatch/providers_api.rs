use super::*;

pub(super) async fn dispatch(
    app: &AppHandle,
    request: &InvokeRequest,
) -> Result<Option<Value>, CommandError> {
    macro_rules! command {
        ($future:expr) => {
            as_json($future.await)
        };
    }
    let result = match request.command.as_str() {
        "list_provider_endpoints" => command!(
            crate::commands::provider_endpoints::list_provider_endpoints(
                app.clone(),
                argument(&request.args, "providerId")?
            )
        ),
        "add_provider_endpoint" => command!(
            crate::commands::provider_endpoints::add_provider_endpoint(
                app.clone(),
                argument(&request.args, "providerId")?,
                argument(&request.args, "url")?,
                argument(&request.args, "expectedFileHash")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        "remove_provider_endpoint" => command!(
            crate::commands::provider_endpoints::remove_provider_endpoint(
                app.clone(),
                argument(&request.args, "providerId")?,
                argument(&request.args, "url")?,
                argument(&request.args, "expectedFileHash")?,
                argument(&request.args, "confirmWrite")?
            )
        ),
        _ => return Ok(None),
    };
    result.map(Some)
}