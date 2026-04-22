use chrono::{DateTime, Utc};
use uuid::Uuid;

use hera_crypto::ReportSignature;
use hera_types::{ChainId, Network};

use crate::{parse_chain, parse_network, DbError, DbPool};

/// Stores immutable artifact references so JSON and PDF outputs can be addressed
/// and audited without fetching object storage contents first.
#[derive(Debug, Clone)]
pub struct StoredArtifactRefs {
    pub json_s3_key: String,
    pub pdf_s3_key: String,
    pub json_sha256: String,
    pub pdf_sha256: String,
}

/// Summarizes a stored report together with the owning case metadata so list
/// endpoints can render downloadable artifacts without extra round trips.
#[derive(Debug, Clone)]
pub struct WorkspaceReportRecord {
    pub case_id: Uuid,
    pub chain: ChainId,
    pub network: Network,
    pub report_status: String,
    pub json_sha256: String,
    pub pdf_sha256: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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

    /// Lists reports for one workspace with case context so frontend grids can
    /// render signed artifacts directly from tenant-scoped data.
    pub async fn list_reports_for_workspace(
        &self,
        workspace_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkspaceReportRecord>, DbError> {
        let rows = sqlx::query_as::<_, WorkspaceReportRow>(
            r#"
            SELECT
                reports.case_id,
                cases.chain,
                cases.network,
                reports.status,
                reports.json_sha256,
                reports.pdf_sha256,
                reports.created_at,
                reports.updated_at
            FROM reports
            INNER JOIN cases ON cases.id = reports.case_id
            WHERE cases.workspace_id = $1
            ORDER BY reports.created_at DESC, reports.case_id DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(workspace_id)
        .bind(
            i64::try_from(limit)
                .map_err(|_| DbError::InvalidData("report list limit overflow".into()))?,
        )
        .bind(
            i64::try_from(offset)
                .map_err(|_| DbError::InvalidData("report list offset overflow".into()))?,
        )
        .fetch_all(self.pool)
        .await
        .map_err(DbError::Query)?;

        rows.into_iter()
            .map(WorkspaceReportRow::try_into_workspace_report)
            .collect()
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ReportRow {
    json_s3_key: String,
    pdf_s3_key: String,
    json_sha256: String,
    pdf_sha256: String,
}

#[derive(Debug, sqlx::FromRow)]
struct WorkspaceReportRow {
    case_id: Uuid,
    chain: String,
    network: String,
    status: String,
    json_sha256: String,
    pdf_sha256: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl WorkspaceReportRow {
    fn try_into_workspace_report(self) -> Result<WorkspaceReportRecord, DbError> {
        Ok(WorkspaceReportRecord {
            case_id: self.case_id,
            chain: parse_chain(&self.chain)?,
            network: parse_network(&self.network)?,
            report_status: self.status,
            json_sha256: self.json_sha256,
            pdf_sha256: self.pdf_sha256,
            created_at: self.created_at,
            updated_at: self.updated_at,
        })
    }
}
