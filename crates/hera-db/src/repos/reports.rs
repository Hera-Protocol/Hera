use uuid::Uuid;

use hera_crypto::ReportSignature;

use crate::{DbError, DbPool};

/// Stores immutable artifact references so JSON and PDF outputs can be addressed
/// and audited without fetching object storage contents first.
#[derive(Debug, Clone)]
pub struct StoredArtifactRefs {
    pub json_s3_key: String,
    pub pdf_s3_key: String,
    pub json_sha256: String,
    pub pdf_sha256: String,
}

/// Persists report metadata and detached signatures so artifact generation stays
/// behind the database boundary instead of hand-writing SQL in the reporter crate.
pub struct ReportRepo<'a> {
    pool: &'a DbPool,
}

impl<'a> ReportRepo<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    /// Upserts the artifact pointers for a case and marks the report row as
    /// signed once both immutable objects have been stored.
    pub async fn upsert_report_artifacts(
        &self,
        case_id: Uuid,
        refs: &StoredArtifactRefs,
    ) -> Result<Uuid, DbError> {
        let report_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO reports (case_id, status, json_s3_key, pdf_s3_key, json_sha256, pdf_sha256)
            VALUES ($1, 'SIGNED', $2, $3, $4, $5)
            ON CONFLICT (case_id)
            DO UPDATE SET
                status = EXCLUDED.status,
                json_s3_key = EXCLUDED.json_s3_key,
                pdf_s3_key = EXCLUDED.pdf_s3_key,
                json_sha256 = EXCLUDED.json_sha256,
                pdf_sha256 = EXCLUDED.pdf_sha256
            RETURNING id
            "#,
        )
        .bind(case_id)
        .bind(&refs.json_s3_key)
        .bind(&refs.pdf_s3_key)
        .bind(&refs.json_sha256)
        .bind(&refs.pdf_sha256)
        .fetch_one(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(report_id)
    }

    /// Stores the detached signature separately from the report metadata so
    /// signature rotation or re-verification can be handled explicitly.
    pub async fn store_report_signature(
        &self,
        report_id: Uuid,
        signature: &ReportSignature,
    ) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO report_signatures (
                report_id, algorithm, public_key_hex, signature_hex, signed_at
            )
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(report_id)
        .bind(&signature.algorithm)
        .bind(&signature.public_key_hex)
        .bind(&signature.signature_hex)
        .bind(signature.signed_at)
        .execute(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(())
    }

    /// Reads the stored artifact references for one case so the API layer can
    /// later stream the right immutable objects to callers.
    pub async fn get_report_artifacts_for_case(
        &self,
        case_id: Uuid,
    ) -> Result<Option<StoredArtifactRefs>, DbError> {
        let row = sqlx::query_as::<_, ReportRow>(
            r#"
            SELECT json_s3_key, pdf_s3_key, json_sha256, pdf_sha256
            FROM reports
            WHERE case_id = $1
            "#,
        )
        .bind(case_id)
        .fetch_optional(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(row.map(|row| StoredArtifactRefs {
            json_s3_key: row.json_s3_key,
            pdf_s3_key: row.pdf_s3_key,
            json_sha256: row.json_sha256,
            pdf_sha256: row.pdf_sha256,
        }))
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ReportRow {
    json_s3_key: String,
    pdf_s3_key: String,
    json_sha256: String,
    pdf_sha256: String,
}
