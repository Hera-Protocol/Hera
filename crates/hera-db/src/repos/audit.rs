use chrono::{DateTime, Utc};
use serde_json::Value;
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
}
