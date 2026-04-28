use axum::{
    extract::{Query, State},
    Extension, Json,
};

use hera_db::repos::workspaces::WorkspaceRepo;
use hera_types::api::{
    CreateWorkspaceRequest, CreateWorkspaceResponse, PaginatedResponse, WorkspaceSummaryResponse,
};

use crate::{
    error::ApiError,
    handlers::PaginationQuery,
    state::{AppState, TenantContext},
};

/// Creates a new workspace owned by the authenticated tenant.
#[tracing::instrument(skip(state))]
pub async fn create_workspace(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Json(payload): Json<CreateWorkspaceRequest>,
) -> Result<(axum::http::StatusCode, Json<CreateWorkspaceResponse>), ApiError> {
    if payload.name.trim().is_empty() {
        return Err(ApiError::BadRequest("workspace name is required".into()));
    }

    let workspace = WorkspaceRepo::new(&state.db)
        .create_workspace(tenant.tenant_id, payload.name.trim())
        .await
        .map_err(ApiError::internal)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreateWorkspaceResponse {
            id: workspace.id,
            name: workspace.name,
        }),
    ))
}

/// Lists workspaces owned by the authenticated tenant.
#[tracing::instrument(skip(state))]
pub async fn list_workspaces(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<WorkspaceSummaryResponse>>, ApiError> {
    let page = query.validate()?;
    let workspaces = WorkspaceRepo::new(&state.db)
        .list_workspaces_for_tenant(tenant.tenant_id, page.limit, page.offset)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(PaginatedResponse {
        items: workspaces
            .into_iter()
            .map(|workspace| WorkspaceSummaryResponse {
                id: workspace.id,
                name: workspace.name,
                created_at: workspace.created_at,
                updated_at: workspace.updated_at,
            })
            .collect(),
        limit: page.limit,
        offset: page.offset,
    }))
}
