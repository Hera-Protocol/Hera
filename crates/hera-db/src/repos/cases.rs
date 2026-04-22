use chrono::{DateTime, Utc};
use hera_types::{Case, ChainId, Network, ScanJobStatus};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    chain_to_db, network_to_db, parse_chain, parse_network, parse_scan_job_status, DbError, DbPool,
};

/// Persists and loads case records while keeping SQL details out of higher layers.
pub struct CaseRepo<'a> {
    pool: &'a DbPool,
}

/// Carries case metadata together with the latest derived scan status so list
/// endpoints can answer in one query rather than forcing N+1 status lookups.
#[derive(Debug, Clone)]
pub struct CaseWithStatus {
    pub case: Case,
    pub scan_status: ScanJobStatus,
}

impl<'a> CaseRepo<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    /// Creates a case with an explicit workspace, chain, and network so every
    /// downstream scan has a well-defined investigative scope.
    pub async fn create_case(
        &self,
        workspace_id: Uuid,
        chain: ChainId,
        network: Network,
        status: &str,
    ) -> Result<Case, DbError> {
        let row = sqlx::query_as::<_, CaseRow>(
            r#"
            INSERT INTO cases (workspace_id, chain, network, status)
            VALUES ($1, $2, $3, $4)
            RETURNING id, workspace_id, chain, network, status, created_at, updated_at
            "#,
        )
        .bind(workspace_id)
        .bind(chain_to_db(&chain))
        .bind(network_to_db(&network))
        .bind(status)
        .fetch_one(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.try_into_case()
    }

    /// Loads one case by id so callers can enforce authorization and orchestration
    /// invariants against a single authoritative database record.
    pub async fn get_case_by_id(&self, case_id: Uuid) -> Result<Option<Case>, DbError> {
        let row = sqlx::query_as::<_, CaseRow>(
            r#"
            SELECT id, workspace_id, chain, network, status, created_at, updated_at
            FROM cases
            WHERE id = $1
            "#,
        )
        .bind(case_id)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.map(CaseRow::try_into_case).transpose()
    }

    /// Loads one case together with its latest scan status so APIs can render a
    /// direct detail page without issuing separate job queries.
    pub async fn get_case_with_status(
        &self,
        case_id: Uuid,
    ) -> Result<Option<CaseWithStatus>, DbError> {
        let row = sqlx::query_as::<_, CaseWithStatusRow>(
            r#"
            SELECT
                cases.id,
                cases.workspace_id,
                cases.chain,
                cases.network,
                cases.status,
                cases.created_at,
                cases.updated_at,
                latest_scan.status AS scan_status,
                latest_scan.failure_reason AS scan_failure_reason
            FROM cases
            LEFT JOIN LATERAL (
                SELECT status, failure_reason
                FROM scan_jobs
                WHERE scan_jobs.case_id = cases.id
                ORDER BY created_at DESC, id DESC
                LIMIT 1
            ) latest_scan ON TRUE
            WHERE cases.id = $1
            "#,
        )
        .bind(case_id)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.map(CaseWithStatusRow::try_into_case_with_status)
            .transpose()
    }

    /// Updates only the case status so higher layers cannot accidentally mutate
    /// scope-defining fields such as workspace, chain, or network.
    pub async fn update_case_status(
        &self,
        case_id: Uuid,
        status: &str,
    ) -> Result<Option<Case>, DbError> {
        let row = sqlx::query_as::<_, CaseRow>(
            r#"
            UPDATE cases
            SET status = $2
            WHERE id = $1
            RETURNING id, workspace_id, chain, network, status, created_at, updated_at
            "#,
        )
        .bind(case_id)
        .bind(status)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.map(CaseRow::try_into_case).transpose()
    }

    /// Lists the cases for one workspace so tenant-scoped APIs can enumerate
    /// exactly the records a workspace owns and nothing broader.
    pub async fn list_cases_for_workspace(&self, workspace_id: Uuid) -> Result<Vec<Case>, DbError> {
        let rows = sqlx::query_as::<_, CaseRow>(
            r#"
            SELECT id, workspace_id, chain, network, status, created_at, updated_at
            FROM cases
            WHERE workspace_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(workspace_id)
        .fetch_all(self.pool)
        .await
        .map_err(DbError::Query)?;

        rows.into_iter().map(CaseRow::try_into_case).collect()
    }

    /// Lists cases for one workspace together with the latest scan status so
    /// overview pages can render useful state without extra queries.
    pub async fn list_cases_with_status_for_workspace(
        &self,
        workspace_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<CaseWithStatus>, DbError> {
        let rows = sqlx::query_as::<_, CaseWithStatusRow>(
            r#"
            SELECT
                cases.id,
                cases.workspace_id,
                cases.chain,
                cases.network,
                cases.status,
                cases.created_at,
                cases.updated_at,
                latest_scan.status AS scan_status,
                latest_scan.failure_reason AS scan_failure_reason
            FROM cases
            LEFT JOIN LATERAL (
                SELECT status, failure_reason
                FROM scan_jobs
                WHERE scan_jobs.case_id = cases.id
                ORDER BY created_at DESC, id DESC
                LIMIT 1
            ) latest_scan ON TRUE
            WHERE cases.workspace_id = $1
            ORDER BY cases.created_at DESC, cases.id DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(workspace_id)
        .bind(
            i64::try_from(limit)
                .map_err(|_| DbError::InvalidData("case list limit overflow".into()))?,
        )
        .bind(
            i64::try_from(offset)
                .map_err(|_| DbError::InvalidData("case list offset overflow".into()))?,
        )
        .fetch_all(self.pool)
        .await
        .map_err(DbError::Query)?;

        rows.into_iter()
            .map(CaseWithStatusRow::try_into_case_with_status)
            .collect()
    }
}

#[derive(Debug, FromRow)]
struct CaseRow {
    id: Uuid,
    workspace_id: Uuid,
    chain: String,
    network: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
struct CaseWithStatusRow {
    id: Uuid,
    workspace_id: Uuid,
    chain: String,
    network: String,
    status: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    scan_status: Option<String>,
    scan_failure_reason: Option<String>,
}

impl CaseRow {
    fn try_into_case(self) -> Result<Case, DbError> {
        Ok(Case {
            id: self.id,
            workspace_id: self.workspace_id,
            chain: parse_chain(&self.chain)?,
            network: parse_network(&self.network)?,
            status: self.status,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}

impl CaseWithStatusRow {
    fn try_into_case_with_status(self) -> Result<CaseWithStatus, DbError> {
        let case = Case {
            id: self.id,
            workspace_id: self.workspace_id,
            chain: parse_chain(&self.chain)?,
            network: parse_network(&self.network)?,
            status: self.status,
            created_at: self.created_at,
            updated_at: self.updated_at,
        };

        let scan_status = match self.scan_status {
            Some(status) => parse_scan_job_status(&status, self.scan_failure_reason)?,
            None => ScanJobStatus::Created,
        };

        Ok(CaseWithStatus { case, scan_status })
    }
}
