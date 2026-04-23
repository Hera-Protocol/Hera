use axum::{
    extract::State,
    http::{header::AUTHORIZATION, Method, Request},
    middleware::Next,
    response::Response,
};

use hera_db::repos::tenancy::TenancyRepo;

use crate::{
    error::ApiError,
    state::{AppState, TenantContext},
};

/// Authenticates the bearer token and injects the tenant context into request
/// extensions so handlers never operate without an authenticated tenant.
pub async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    if request.method() == Method::OPTIONS {
        return Ok(next.run(request).await);
    }

    let header_value = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .ok_or(ApiError::Unauthorized("missing authorization header"))?;
    let token = header_value
        .strip_prefix("Bearer ")
        .ok_or(ApiError::Unauthorized("expected bearer token"))?;

    let tenant = TenancyRepo::new(&state.db)
        .find_tenant_by_api_key(token)
        .await
        .map_err(ApiError::internal)?
        .ok_or(ApiError::Unauthorized("invalid api key"))?;

    request.extensions_mut().insert(TenantContext {
        tenant_id: tenant.id,
    });

    Ok(next.run(request).await)
}
