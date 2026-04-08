use axum::{extract::State, Extension, Json};
use serde::{Deserialize, Serialize};

use hera_db::repos::workspaces::WorkspaceRepo;

use crate::{
    error::ApiError,
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateWorkspaceRequest {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CreateWorkspaceResponse {
    pub id: uuid::Uuid,
    pub name: String,
}
