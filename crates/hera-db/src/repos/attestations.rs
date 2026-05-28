use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::{DbError, DbPool};

#[derive(Debug, Clone)]
pub struct StoredAttestation {
    pub id: Uuid,
    pub case_id: Uuid,
    pub job_id: Uuid,
    pub proof_type: String,
    pub proof_s3_key: String,
    pub proof_sha256: String,
    pub public_inputs_json: serde_json::Value,
    pub srs_version: String,
    pub created_at: DateTime<Utc>,
}

pub struct AttestationRepo<'a> {
    pool: &'a DbPool,
}

impl<'a> AttestationRepo<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    pub async fn store_attestation(
        &self,
        case_id: Uuid,
        job_id: Uuid,
        proof_type: &str,
        proof_s3_key: &str,
        proof_sha256: &str,
        public_inputs_json: &serde_json::Value,
        srs_version: &str,
    ) -> Result<Uuid, DbError> {
        let id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO attestations (
                case_id, job_id, proof_type, proof_s3_key, proof_sha256,
                public_inputs_json, srs_version
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id
            "#,
        )
        .bind(case_id)
        .bind(job_id)
        .bind(proof_type)
        .bind(proof_s3_key)
        .bind(proof_sha256)
        .bind(public_inputs_json)
        .bind(srs_version)
        .fetch_one(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(id)
    }

    pub async fn get_for_case(
        &self,
        case_id: Uuid,
    ) -> Result<Vec<StoredAttestation>, DbError> {
        let rows = sqlx::query_as::<_, AttestationRow>(
            r#"
            SELECT id, case_id, job_id, proof_type, proof_s3_key, proof_sha256,
                   public_inputs_json, srs_version, created_at
            FROM attestations
            WHERE case_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(case_id)
        .fetch_all(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(rows.into_iter().map(AttestationRow::into_stored).collect())
    }

    pub async fn get_by_job_id(
        &self,
        job_id: Uuid,
    ) -> Result<Option<StoredAttestation>, DbError> {
        let row = sqlx::query_as::<_, AttestationRow>(
            r#"
            SELECT id, case_id, job_id, proof_type, proof_s3_key, proof_sha256,
                   public_inputs_json, srs_version, created_at
            FROM attestations
            WHERE job_id = $1
            "#,
        )
        .bind(job_id)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(row.map(AttestationRow::into_stored))
    }
}

#[derive(Debug, FromRow)]
struct AttestationRow {
    id: Uuid,
    case_id: Uuid,
    job_id: Uuid,
    proof_type: String,
    proof_s3_key: String,
    proof_sha256: String,
    public_inputs_json: serde_json::Value,
    srs_version: String,
    created_at: DateTime<Utc>,
}

impl AttestationRow {
    fn into_stored(self) -> StoredAttestation {
        StoredAttestation {
            id: self.id,
            case_id: self.case_id,
            job_id: self.job_id,
            proof_type: self.proof_type,
            proof_s3_key: self.proof_s3_key,
            proof_sha256: self.proof_sha256,
            public_inputs_json: self.public_inputs_json,
            srs_version: self.srs_version,
            created_at: self.created_at,
        }
    }
}
