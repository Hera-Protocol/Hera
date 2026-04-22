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

    /// Lists workspaces for one tenant in a deterministic order so UIs can page
    /// through tenant scopes without scanning unrelated records.
    pub async fn list_workspaces_for_tenant(
        &self,
        tenant_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkspaceRecord>, DbError> {
        let rows = sqlx::query_as::<_, WorkspaceRow>(
            r#"
            SELECT id, tenant_id, name, created_at, updated_at
            FROM workspaces
            WHERE tenant_id = $1
            ORDER BY created_at DESC, id DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(tenant_id)
        .bind(
            i64::try_from(limit)
                .map_err(|_| DbError::InvalidData("workspace list limit overflow".into()))?,
        )
        .bind(
            i64::try_from(offset)
                .map_err(|_| DbError::InvalidData("workspace list offset overflow".into()))?,
        )
        .fetch_all(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(rows
            .into_iter()
            .map(|row| WorkspaceRecord {
                id: row.id,
                tenant_id: row.tenant_id,
                name: row.name,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
            .collect())
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
