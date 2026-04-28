use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use uuid::Uuid;

use hera_db::repos::{
    audit::{AuditAction, AuditEntry},
    jobs::ScanJobRepo,
    reports::ReportRepo,
    tenancy::TenancyRepo,
};
use hera_types::{
    api::{PaginatedResponse, WorkspaceReportResponse},
    ScanJobStatus,
};

use crate::{
    error::ApiError,
    handlers::{append_audit, PaginationQuery},
    state::{AppState, TenantContext},
};

/// Returns the signed JSON report once the case has reached SIGNED.
#[tracing::instrument(skip(state))]
pub async fn report_json(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
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
    append_audit(
        &state,
        AuditEntry {
            actor_id: tenant.tenant_id,
            action: AuditAction::ReportExported,
            resource_id: case_id,
            resource_type: "case".to_string(),
            ip_addr: None,
            metadata: serde_json::json!({
                "case_id": case_id,
                "format": "json",
                "sha256": artifacts.json_sha256.clone(),
            }),
            occurred_at: None,
        },
    )
    .await?;

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
    Extension(tenant): Extension<TenantContext>,
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
    append_audit(
        &state,
        AuditEntry {
            actor_id: tenant.tenant_id,
            action: AuditAction::ReportExported,
            resource_id: case_id,
            resource_type: "case".to_string(),
            ip_addr: None,
            metadata: serde_json::json!({
                "case_id": case_id,
                "format": "pdf",
                "sha256": artifacts.pdf_sha256.clone(),
            }),
            occurred_at: None,
        },
    )
    .await?;

    Ok(artifact_response(
        StatusCode::OK,
        "application/pdf",
        &artifacts.pdf_sha256,
        body,
    ))
}

/// Lists reports for one workspace.
#[tracing::instrument(skip(state))]
pub async fn list_workspace_reports(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<WorkspaceReportResponse>>, ApiError> {
    let page = query.validate()?;
    let belongs = TenancyRepo::new(&state.db)
        .workspace_belongs_to_tenant(workspace_id, tenant.tenant_id)
        .await
        .map_err(ApiError::internal)?;
    if !belongs {
        return Err(ApiError::Forbidden("workspace does not belong to tenant"));
    }

    let reports = ReportRepo::new(&state.db)
        .list_reports_for_workspace(workspace_id, page.limit, page.offset)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(PaginatedResponse {
        items: reports
            .into_iter()
            .map(|report| WorkspaceReportResponse {
                case_id: report.case_id,
                chain: report.chain,
                network: report.network,
                status: report.report_status,
                json_sha256: report.json_sha256,
                pdf_sha256: report.pdf_sha256,
                created_at: report.created_at,
                updated_at: report.updated_at,
            })
            .collect(),
        limit: page.limit,
        offset: page.offset,
    }))
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
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Ok(value) = HeaderValue::from_str(sha256) {
        response.headers_mut().insert("x-artifact-sha256", value);
    }
    response
}
