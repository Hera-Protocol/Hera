use axum::{extract::State, http::Request, middleware::Next, response::Response};
use uuid::Uuid;

use hera_db::repos::tenancy::TenancyRepo;

use crate::{
    error::ApiError,
    state::{AppState, TenantContext},
};

/// Enforces tenant ownership at the middleware layer so individual handlers
/// don't need to re-check it. A handler that forgets to check would still be safe.
pub async fn tenant_isolation(
    State(state): State<AppState>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, ApiError> {
    let tenant = request
        .extensions()
        .get::<TenantContext>()
        .ok_or(ApiError::Unauthorized("missing tenant context"))?
        .clone();

    let path = request.uri().path().to_string();
    if let Some(workspace_id) = extract_workspace_id(&path) {
        let allowed = TenancyRepo::new(&state.db)
            .workspace_belongs_to_tenant(workspace_id, tenant.tenant_id)
            .await
            .map_err(ApiError::internal)?;
        if !allowed {
            return Err(ApiError::Forbidden("cross-tenant workspace access denied"));
        }
    }

    if let Some(case_id) = extract_case_id(&path) {
        let allowed = TenancyRepo::new(&state.db)
            .case_belongs_to_tenant(case_id, tenant.tenant_id)
            .await
            .map_err(ApiError::internal)?;
        if !allowed {
            return Err(ApiError::Forbidden("cross-tenant case access denied"));
        }
    }

    Ok(next.run(request).await)
}

fn extract_workspace_id(path: &str) -> Option<Uuid> {
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    let api_version = segments.next()?;
    let resource = segments.next()?;
    let id = segments.next()?;

    if api_version == "v1" && resource == "workspaces" {
        Uuid::parse_str(id).ok()
    } else {
        None
    }
}

fn extract_case_id(path: &str) -> Option<Uuid> {
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    let api_version = segments.next()?;
    let resource = segments.next()?;
    let id = segments.next()?;

    if api_version == "v1" && resource == "cases" {
        Uuid::parse_str(id).ok()
    } else {
        None
    }
}
