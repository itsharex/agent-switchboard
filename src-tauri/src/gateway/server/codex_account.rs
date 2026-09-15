//! Resolve the official managed account for one Codex request; never mutate
//! the saved route. Codex-owned: the shape mirrors no Claude module, and the
//! refresh/identity semantics live in `codex_auth`.

use super::*;
use tiny_http::Header;

/// Replaces the saved route's placeholder credential with fresh managed
/// tokens for every attempt. Refresh failures fail the request without ever
/// being classified as an upstream provider failure.
pub(super) fn resolve(
    candidate: &ActiveRoute,
    inner: &GatewayInner,
    incoming: Option<&[Header]>,
) -> Result<ActiveRoute, (u16, String)> {
    let Some(bound) = &candidate.codex_account else {
        return Ok(candidate.clone());
    };
    if let Some(headers) = incoming {
        verify_native_identity(headers, candidate, &bound.account_id)?;
    }
    let auth_path =
        crate::local_state::LocalState::codex_auth_path().map_err(|error| (500, error))?;
    let fresh = crate::codex_auth::projection::resolved_managed_auth(
        &inner.state_root,
        &bound.managed_id,
        &auth_path,
    )
    .map_err(|error| (401, error))?;
    let mut route = candidate.clone();
    route.api_key = fresh.access_token.clone();
    route.codex_account = Some(fresh);
    Ok(route)
}

/// The native CLI login must belong to the bound account. The route's own
/// capability token is always accepted; anything else must carry that
/// account's ChatGPT identity claim, so a CLI logged into another account is
/// rejected instead of silently spending the managed quota.
fn verify_native_identity(
    headers: &[Header],
    route: &ActiveRoute,
    account_id: &str,
) -> Result<(), (u16, String)> {
    let Some(value) = headers
        .iter()
        .find(|header| {
            header
                .field
                .as_str()
                .as_str()
                .eq_ignore_ascii_case("authorization")
        })
        .map(|header| header.value.as_str())
    else {
        return Ok(());
    };
    let token = value.strip_prefix("Bearer ").unwrap_or(value).trim();
    if token.is_empty() || token == route.client_token {
        return Ok(());
    }
    match crate::codex_auth::projection::token_account_id(token) {
        Some(claimed) if claimed == account_id => Ok(()),
        _ => Err((
            403,
            "Codex 本地登录与绑定的托管账号身份不一致，已拒绝本次接管请求".to_string(),
        )),
    }
}

/// The official `/models` document served verbatim to a taken-over official
/// client: official model identity belongs to the backend, not a catalog.
pub(super) fn official_models_document(
    inner: &GatewayInner,
    route: &ActiveRoute,
) -> Result<String, (u16, String)> {
    let Some(bound) = &route.codex_account else {
        return Err((500, "官方模型查询缺少绑定的托管账号".to_string()));
    };
    let auth_path =
        crate::local_state::LocalState::codex_auth_path().map_err(|error| (500, error))?;
    crate::codex_auth::official_models_document(&inner.state_root, &bound.managed_id, &auth_path)
        .map_err(|error| (502, error))
}
