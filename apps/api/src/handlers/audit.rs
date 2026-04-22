use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use hera_db::repos::{audit::AuditRepo, tenancy::TenancyRepo};

use crate::{
    error::ApiError,
    handlers::{PaginatedResponse, PaginationQuery},
    state::{AppState, TenantContext},
};

/// Lists audit log rows attributable to one workspace.
#[tracing::instrument(skip(state))]
pub async fn list_workspace_audit_logs(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<AuditLogResponse>>, ApiError> {
    let page = query.validate()?;
    let belongs = TenancyRepo::new(&state.db)
        .workspace_belongs_to_tenant(workspace_id, tenant.tenant_id)
        .await
        .map_err(ApiError::internal)?;
    if !belongs {
        return Err(ApiError::Forbidden("workspace does not belong to tenant"));
    }

    let logs = AuditRepo::new(&state.db)
        .list_audit_logs_for_workspace(workspace_id, page.limit, page.offset)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(PaginatedResponse {
        items: logs
            .into_iter()
            .map(|log| AuditLogResponse {
                id: log.id,
                actor_id: log.actor_id,
                action: log.action,
                resource_id: log.resource_id,
                resource_type: log.resource_type,
                ip_addr: log.ip_addr,
                metadata: log.metadata,
                created_at: log.created_at,
                updated_at: log.updated_at,
            })
            .collect(),
        limit: page.limit,
        offset: page.offset,
    }))
}

#[derive(Debug, Serialize)]
pub struct AuditLogResponse {
    pub id: Uuid,
    pub actor_id: Uuid,
    pub action: String,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub ip_addr: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
