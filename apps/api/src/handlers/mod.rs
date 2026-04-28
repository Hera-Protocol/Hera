use serde::Deserialize;

use hera_db::repos::audit::{AuditEntry, AuditRepo};

use crate::error::ApiError;
use crate::state::AppState;

pub mod audit;
pub mod cases;
pub mod keys;
pub mod reports;
pub mod workspaces;

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct Pagination {
    pub limit: usize,
    pub offset: usize,
}

impl PaginationQuery {
    pub fn validate(self) -> Result<Pagination, ApiError> {
        let limit = self.limit.unwrap_or(50);
        let offset = self.offset.unwrap_or(0);
        if limit == 0 {
            return Err(ApiError::BadRequest(
                "limit must be greater than zero".into(),
            ));
        }
        if limit > 100 {
            return Err(ApiError::BadRequest(
                "limit must be less than or equal to 100".into(),
            ));
        }

        Ok(Pagination { limit, offset })
    }
}

pub async fn append_audit(state: &AppState, entry: AuditEntry) -> Result<(), ApiError> {
    AuditRepo::new(&state.db)
        .append_audit_log(entry)
        .await
        .map_err(ApiError::internal)
}
