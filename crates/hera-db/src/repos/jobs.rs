use chrono::{DateTime, Utc};
use hera_types::{ChainId, Network, ScanJob, ScanJobStatus};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{
    chain_to_db, network_to_db, parse_chain, parse_network, parse_scan_job_status,
    scan_job_status_to_db, DbError, DbPool,
};

/// Persists scan job control-plane state so worker processes coordinate through
/// durable records rather than in-memory assumptions.
pub struct ScanJobRepo<'a> {
    pool: &'a DbPool,
}

impl<'a> ScanJobRepo<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    /// Creates a scan job bound to one case so queue messages always point to a
    /// durable record with chain and network context.
    pub async fn create_scan_job(
        &self,
        case_id: Uuid,
        chain: ChainId,
        network: Network,
        status: ScanJobStatus,
    ) -> Result<ScanJob, DbError> {
        let (status_value, failure_reason) = scan_job_status_to_db(&status);
        let row = sqlx::query_as::<_, ScanJobRow>(
            r#"
            INSERT INTO scan_jobs (case_id, chain, network, status, failure_reason)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, case_id, chain, network, status, failure_reason, created_at, updated_at
            "#,
        )
        .bind(case_id)
        .bind(chain_to_db(&chain))
        .bind(network_to_db(&network))
        .bind(status_value)
        .bind(failure_reason)
        .fetch_one(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.try_into_scan_job()
    }

    /// Loads one scan job by id so the orchestrator can drive the state machine
    /// from a single authoritative database record.
    pub async fn get_scan_job_by_id(&self, job_id: Uuid) -> Result<Option<ScanJob>, DbError> {
        let row = sqlx::query_as::<_, ScanJobRow>(
            r#"
            SELECT id, case_id, chain, network, status, failure_reason, created_at, updated_at
            FROM scan_jobs
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.map(ScanJobRow::try_into_scan_job).transpose()
    }

    /// Updates only the job status because each state transition must remain
    /// explicit and auditable.
    pub async fn update_scan_job_status(
        &self,
        job_id: Uuid,
        status: ScanJobStatus,
    ) -> Result<Option<ScanJob>, DbError> {
        let (status_value, failure_reason) = scan_job_status_to_db(&status);
        let row = sqlx::query_as::<_, ScanJobRow>(
            r#"
            UPDATE scan_jobs
            SET status = $2, failure_reason = $3
            WHERE id = $1
            RETURNING id, case_id, chain, network, status, failure_reason, created_at, updated_at
            "#,
        )
        .bind(job_id)
        .bind(status_value)
        .bind(failure_reason)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.map(ScanJobRow::try_into_scan_job).transpose()
    }

    /// Retrieves the most recent scan job for a case so status APIs can answer
    /// from the latest orchestration attempt without custom query logic.
    pub async fn get_latest_scan_job_for_case(
        &self,
        case_id: Uuid,
    ) -> Result<Option<ScanJob>, DbError> {
        let row = sqlx::query_as::<_, ScanJobRow>(
            r#"
            SELECT id, case_id, chain, network, status, failure_reason, created_at, updated_at
            FROM scan_jobs
            WHERE case_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(case_id)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.map(ScanJobRow::try_into_scan_job).transpose()
    }
}

#[derive(Debug, FromRow)]
struct ScanJobRow {
    id: Uuid,
    case_id: Uuid,
    chain: String,
    network: String,
    status: String,
    failure_reason: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl ScanJobRow {
    fn try_into_scan_job(self) -> Result<ScanJob, DbError> {
        Ok(ScanJob {
            id: self.id,
            case_id: self.case_id,
            chain: parse_chain(&self.chain)?,
            network: parse_network(&self.network)?,
            status: parse_scan_job_status(&self.status, self.failure_reason)?,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}
