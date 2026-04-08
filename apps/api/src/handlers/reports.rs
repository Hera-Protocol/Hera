use axum::extract::{Path, State};
use uuid::Uuid;

use hera_db::repos::jobs::ScanJobRepo;
use hera_types::ScanJobStatus;

use crate::error::ApiError;
use crate::state::AppState;

/// Returns the signed JSON report once the case has reached SIGNED.
#[tracing::instrument(skip(state))]
pub async fn report_json(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<axum::response::Response, ApiError> {
    ensure_signed(&state, case_id).await?;
    Err(ApiError::Conflict(
        "report artifacts will be streamed once apps/reporter storage is wired".into(),
    ))
}

/// Returns the signed PDF report once the case has reached SIGNED.
#[tracing::instrument(skip(state))]
pub async fn report_pdf(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<axum::response::Response, ApiError> {
    ensure_signed(&state, case_id).await?;
    Err(ApiError::Conflict(
        "report artifacts will be streamed once apps/reporter storage is wired".into(),
    ))
}

async fn ensure_signed(state: &AppState, case_id: Uuid) -> Result<(), ApiError> {
    let status = ScanJobRepo::new(&state.db)
        .get_latest_scan_job_for_case(case_id)
        .await
        .map_err(ApiError::internal)?
        .map(|job| job.status)
        .unwrap_or(ScanJobStatus::Created);

    if matches!(status, ScanJobStatus::Signed) {
        Ok(())
    } else {
        Err(ApiError::Conflict(format!(
            "report is not available while case status is {:?}",
            status
        )))
    }
}
