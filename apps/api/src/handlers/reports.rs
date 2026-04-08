use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use hera_db::repos::{jobs::ScanJobRepo, reports::ReportRepo};
use hera_types::ScanJobStatus;

use crate::error::ApiError;
use crate::state::AppState;

/// Returns the signed JSON report once the case has reached SIGNED.
#[tracing::instrument(skip(state))]
pub async fn report_json(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    ensure_signed(&state, case_id).await?;
    let artifacts = ReportRepo::new(&state.db)
        .get_report_artifacts_for_case(case_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or(ApiError::NotFound("report artifacts not found"))?;
    let body = state
        .report_storage
        .load_artifact_bytes(&artifacts.json_s3_key)
        .await
        .map_err(ApiError::internal)?;

    Ok(artifact_response(
        StatusCode::OK,
        "application/json",
        &artifacts.json_sha256,
        body,
    ))
}

/// Returns the signed PDF report once the case has reached SIGNED.
#[tracing::instrument(skip(state))]
pub async fn report_pdf(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Response, ApiError> {
    ensure_signed(&state, case_id).await?;
    let artifacts = ReportRepo::new(&state.db)
        .get_report_artifacts_for_case(case_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or(ApiError::NotFound("report artifacts not found"))?;
    let body = state
        .report_storage
        .load_artifact_bytes(&artifacts.pdf_s3_key)
        .await
        .map_err(ApiError::internal)?;

    Ok(artifact_response(
        StatusCode::OK,
        "application/pdf",
        &artifacts.pdf_sha256,
        body,
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

fn artifact_response(
    status: StatusCode,
    content_type: &'static str,
    sha256: &str,
    body: Vec<u8>,
) -> Response {
    let mut response = (status, body).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(content_type),
    );
    if let Ok(value) = HeaderValue::from_str(sha256) {
        response
            .headers_mut()
            .insert("x-artifact-sha256", value);
    }
    response
}
