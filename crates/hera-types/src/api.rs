use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{ChainId, Network, ScanJobStatus};

/// Generic paginated API response shared by the server and SDK clients.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PaginatedResponse<T> {
    pub items: Vec<T>,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateWorkspaceRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CreateWorkspaceResponse {
    pub id: Uuid,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceSummaryResponse {
    pub id: Uuid,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CreateCaseRequest {
    pub workspace_id: Uuid,
    pub chain: ChainId,
    pub network: Network,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CreateCaseResponse {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CaseSummaryResponse {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub chain: ChainId,
    pub network: Network,
    pub case_status: String,
    pub scan_status: ScanJobStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CaseDetailResponse {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub chain: ChainId,
    pub network: Network,
    pub case_status: String,
    pub scan_status: ScanJobStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_checkpoint: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ScanCaseResponse {
    pub job_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct GetCaseStatusResponse {
    pub status: ScanJobStatus,
    pub last_checkpoint: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ImportViewingKeyRequest {
    pub raw_key: String,
    pub birthday_height: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ImportViewingKeyResponse {
    pub key_ref: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceViewKeyResponse {
    pub id: Uuid,
    pub case_id: Uuid,
    pub chain: ChainId,
    pub key_ref: String,
    pub birthday_height: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkspaceReportResponse {
    pub case_id: Uuid,
    pub chain: ChainId,
    pub network: Network,
    pub status: String,
    pub json_sha256: String,
    pub pdf_sha256: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AuditLogResponse {
    pub id: Uuid,
    pub actor_id: Uuid,
    pub action: String,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub ip_addr: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
