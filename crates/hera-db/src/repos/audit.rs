use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{types::Json, FromRow};
use uuid::Uuid;

use crate::{DbError, DbPool};

/// Enumerates auditable actions so security-critical reads and writes are stored
/// consistently and can be filtered without free-form string drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditAction {
    KeyRead,
    KeyImport,
    ScanStarted,
    ReportExported,
    StatusChanged,
    CaseCreated,
}

impl AuditAction {
    fn as_db_value(&self) -> &'static str {
        match self {
            AuditAction::KeyRead => "KEY_READ",
            AuditAction::KeyImport => "KEY_IMPORT",
            AuditAction::ScanStarted => "SCAN_STARTED",
            AuditAction::ReportExported => "REPORT_EXPORTED",
            AuditAction::StatusChanged => "STATUS_CHANGED",
            AuditAction::CaseCreated => "CASE_CREATED",
        }
    }
}

/// Represents one immutable audit event because compliance requires a durable
/// record of every sensitive action taken by users or services.
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub actor_id: Uuid,
    pub action: AuditAction,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub ip_addr: Option<String>,
    pub metadata: Value,
    pub occurred_at: Option<DateTime<Utc>>,
}

/// Represents an immutable audit record returned to API clients.
#[derive(Debug, Clone)]
pub struct AuditLogRecord {
    pub id: Uuid,
    pub actor_id: Uuid,
    pub action: String,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub ip_addr: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Persists append-only audit records. Every read, write, scan, and export must
/// produce an audit log entry. This is not optional — it is a core compliance requirement.
pub struct AuditRepo<'a> {
    pool: &'a DbPool,
}

impl<'a> AuditRepo<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    /// Inserts one audit log row and never updates prior audit history.
    pub async fn append_audit_log(&self, entry: AuditEntry) -> Result<(), DbError> {
        let AuditEntry {
            actor_id,
            action,
            resource_id,
            resource_type,
            ip_addr,
            metadata,
            occurred_at,
        } = entry;

        sqlx::query(
            r#"
            INSERT INTO audit_logs (
                actor_id,
                action,
                resource_id,
                resource_type,
                ip_addr,
                metadata,
                created_at,
                updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, COALESCE($7, NOW()), COALESCE($7, NOW()))
            "#,
        )
        .bind(actor_id)
        .bind(action.as_db_value())
        .bind(resource_id)
        .bind(resource_type)
        .bind(ip_addr)
        .bind(metadata)
        .bind(occurred_at)
        .execute(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(())
    }

    /// Lists audit rows attributable to one workspace by resolving the resource
    /// boundary through workspace, case, or scan-job ownership.
    pub async fn list_audit_logs_for_workspace(
        &self,
        workspace_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<AuditLogRecord>, DbError> {
        let rows = sqlx::query_as::<_, AuditLogRow>(
            r#"
            SELECT
                audit_logs.id,
                audit_logs.actor_id,
                audit_logs.action,
                audit_logs.resource_id,
                audit_logs.resource_type,
                audit_logs.ip_addr,
                audit_logs.metadata,
                audit_logs.created_at,
                audit_logs.updated_at
            FROM audit_logs
            WHERE (
                audit_logs.resource_type = 'workspace'
                AND EXISTS (
                    SELECT 1
                    FROM workspaces
                    WHERE workspaces.id = audit_logs.resource_id
                      AND workspaces.id = $1
                )
            ) OR (
                audit_logs.resource_type = 'case'
                AND EXISTS (
                    SELECT 1
                    FROM cases
                    WHERE cases.id = audit_logs.resource_id
                      AND cases.workspace_id = $1
                )
            ) OR (
                audit_logs.resource_type = 'scan_job'
                AND EXISTS (
                    SELECT 1
                    FROM scan_jobs
                    INNER JOIN cases ON cases.id = scan_jobs.case_id
                    WHERE scan_jobs.id = audit_logs.resource_id
                      AND cases.workspace_id = $1
                )
            )
            ORDER BY audit_logs.created_at DESC, audit_logs.id DESC
            LIMIT $2 OFFSET $3
            "#,
        )
        .bind(workspace_id)
        .bind(
            i64::try_from(limit)
                .map_err(|_| DbError::InvalidData("audit log limit overflow".into()))?,
        )
        .bind(
            i64::try_from(offset)
                .map_err(|_| DbError::InvalidData("audit log offset overflow".into()))?,
        )
        .fetch_all(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(rows
            .into_iter()
            .map(|row| AuditLogRecord {
                id: row.id,
                actor_id: row.actor_id,
                action: row.action,
                resource_id: row.resource_id,
                resource_type: row.resource_type,
                ip_addr: row.ip_addr,
                metadata: row.metadata.0,
                created_at: row.created_at,
                updated_at: row.updated_at,
            })
            .collect())
    }
}

#[derive(Debug, FromRow)]
struct AuditLogRow {
    id: Uuid,
    actor_id: Uuid,
    action: String,
    resource_id: Uuid,
    resource_type: String,
    ip_addr: Option<String>,
    metadata: Json<Value>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
