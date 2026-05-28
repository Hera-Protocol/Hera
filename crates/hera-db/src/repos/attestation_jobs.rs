use chrono::{DateTime, Utc};
use hera_types::{AttestationJob, AttestationJobStatus};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{attestation_job_status_to_db, parse_attestation_job_status, DbError, DbPool};

pub struct AttestationJobRepo<'a> {
    pool: &'a DbPool,
}

impl<'a> AttestationJobRepo<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    pub async fn create(
        &self,
        case_id: Uuid,
        proof_type: &str,
        parameters: &serde_json::Value,
    ) -> Result<AttestationJob, DbError> {
        let (status_value, failure_reason) =
            attestation_job_status_to_db(&AttestationJobStatus::Created);
        let row = sqlx::query_as::<_, AttestationJobRow>(
            r#"
            INSERT INTO attestation_jobs (case_id, proof_type, status, failure_reason, parameters)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, case_id, proof_type, status, failure_reason, parameters, created_at, updated_at
            "#,
        )
        .bind(case_id)
        .bind(proof_type)
        .bind(status_value)
        .bind(failure_reason)
        .bind(parameters)
        .fetch_one(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.try_into_attestation_job()
    }

    pub async fn get_by_id(&self, job_id: Uuid) -> Result<Option<AttestationJob>, DbError> {
        let row = sqlx::query_as::<_, AttestationJobRow>(
            r#"
            SELECT id, case_id, proof_type, status, failure_reason, parameters, created_at, updated_at
            FROM attestation_jobs
            WHERE id = $1
            "#,
        )
        .bind(job_id)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.map(AttestationJobRow::try_into_attestation_job)
            .transpose()
    }

    pub async fn update_status(
        &self,
        job_id: Uuid,
        status: AttestationJobStatus,
    ) -> Result<Option<AttestationJob>, DbError> {
        let (status_value, failure_reason) = attestation_job_status_to_db(&status);
        let row = sqlx::query_as::<_, AttestationJobRow>(
            r#"
            UPDATE attestation_jobs
            SET status = $2, failure_reason = $3
            WHERE id = $1
            RETURNING id, case_id, proof_type, status, failure_reason, parameters, created_at, updated_at
            "#,
        )
        .bind(job_id)
        .bind(status_value)
        .bind(failure_reason)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.map(AttestationJobRow::try_into_attestation_job)
            .transpose()
    }

    pub async fn get_latest_for_case(
        &self,
        case_id: Uuid,
    ) -> Result<Option<AttestationJob>, DbError> {
        let row = sqlx::query_as::<_, AttestationJobRow>(
            r#"
            SELECT id, case_id, proof_type, status, failure_reason, parameters, created_at, updated_at
            FROM attestation_jobs
            WHERE case_id = $1
            ORDER BY created_at DESC
            LIMIT 1
            "#,
        )
        .bind(case_id)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        row.map(AttestationJobRow::try_into_attestation_job)
            .transpose()
    }
}

#[derive(Debug, FromRow)]
struct AttestationJobRow {
    id: Uuid,
    case_id: Uuid,
    proof_type: String,
    status: String,
    failure_reason: Option<String>,
    parameters: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AttestationJobRow {
    fn try_into_attestation_job(self) -> Result<AttestationJob, DbError> {
        Ok(AttestationJob {
            id: self.id,
            case_id: self.case_id,
            proof_type: self.proof_type,
            status: parse_attestation_job_status(&self.status, self.failure_reason)?,
            failure_reason: None,
            parameters: self.parameters,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}
