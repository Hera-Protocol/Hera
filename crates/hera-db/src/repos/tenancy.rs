use sqlx::FromRow;
use uuid::Uuid;

use crate::{DbError, DbPool};

/// Carries the authenticated tenant identity extracted from an API key so the
/// HTTP layer can enforce isolation without direct SQL access.
#[derive(Debug, Clone)]
pub struct TenantRecord {
    pub id: Uuid,
    pub name: String,
}

/// Centralizes tenant lookup and ownership checks so authorization rules stay in
/// one persistence boundary instead of being duplicated in handlers.
pub struct TenancyRepo<'a> {
    pool: &'a DbPool,
}

impl<'a> TenancyRepo<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    /// Resolves a tenant from the presented API key so every authenticated
    /// request carries a durable tenant identity.
    pub async fn find_tenant_by_api_key(
        &self,
        api_key: &str,
    ) -> Result<Option<TenantRecord>, DbError> {
        let row = sqlx::query_as::<_, TenantRow>(
            r#"
            SELECT id, name
            FROM tenants
            WHERE api_key_hash = $1
            "#,
        )
        .bind(api_key)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(row.map(|row| TenantRecord {
            id: row.id,
            name: row.name,
        }))
    }

    /// Checks case ownership through the workspace relationship so case-scoped
    /// routes cannot cross tenant boundaries even if a handler forgets to check.
    pub async fn case_belongs_to_tenant(
        &self,
        case_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<bool, DbError> {
        let row = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM cases
                INNER JOIN workspaces ON workspaces.id = cases.workspace_id
                WHERE cases.id = $1 AND workspaces.tenant_id = $2
            )
            "#,
        )
        .bind(case_id)
        .bind(tenant_id)
        .fetch_one(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(row)
    }

    /// Checks workspace ownership directly so workspace-scoped endpoints can be
    /// denied before handler code runs.
    pub async fn workspace_belongs_to_tenant(
        &self,
        workspace_id: Uuid,
        tenant_id: Uuid,
    ) -> Result<bool, DbError> {
        let row = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM workspaces
                WHERE id = $1 AND tenant_id = $2
            )
            "#,
        )
        .bind(workspace_id)
        .bind(tenant_id)
        .fetch_one(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(row)
    }
}

#[derive(Debug, FromRow)]
struct TenantRow {
    id: Uuid,
    name: String,
}
