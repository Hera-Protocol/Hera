use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{DbError, DbPool};

/// Represents a tenant-owned workspace record for API creation flows.
#[derive(Debug, Clone)]
pub struct WorkspaceRecord {
    pub id: Uuid,
    pub tenant_id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Persists workspace records so the API can create tenant-owned scopes without
/// embedding SQL in handler code.
pub struct WorkspaceRepo<'a> {
    pool: &'a DbPool,
}

impl<'a> WorkspaceRepo<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    /// Creates a workspace owned by one tenant so every case is later anchored
    /// to an explicit tenant boundary.
    pub async fn create_workspace(
        &self,
        tenant_id: Uuid,
        name: &str,
    ) -> Result<WorkspaceRecord, DbError> {
        let row = sqlx::query_as::<_, WorkspaceRow>(
            r#"
            INSERT INTO workspaces (tenant_id, name)
            VALUES ($1, $2)
            RETURNING id, tenant_id, name, created_at, updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(name)
        .fetch_one(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(WorkspaceRecord {
            id: row.id,
            tenant_id: row.tenant_id,
            name: row.name,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct WorkspaceRow {
    id: Uuid,
    tenant_id: Uuid,
    name: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
