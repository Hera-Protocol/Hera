use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use uuid::Uuid;

use hera_db::repos::{
    audit::{AuditAction, AuditEntry},
    cases::CaseRepo,
    checkpoints::CheckpointRepo,
    events::EventRepo,
    jobs::ScanJobRepo,
    tenancy::TenancyRepo,
};
use hera_types::{
    api::{
        CaseDetailResponse, CaseSummaryResponse, CreateCaseRequest, CreateCaseResponse,
        GetCaseStatusResponse, PaginatedResponse, ScanCaseResponse,
    },
    CanonicalEvent, ScanJobStatus,
};
use hera_worker::jobs::scan_job::{enqueue, ScanJobMessage};

use crate::{
    error::ApiError,
    handlers::{append_audit, PaginationQuery},
    state::{AppState, TenantContext},
};

/// Creates a new case under an existing workspace owned by the authenticated tenant.
#[tracing::instrument(skip(state))]
pub async fn create_case(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Json(payload): Json<CreateCaseRequest>,
) -> Result<(axum::http::StatusCode, Json<CreateCaseResponse>), ApiError> {
    let belongs = TenancyRepo::new(&state.db)
        .workspace_belongs_to_tenant(payload.workspace_id, tenant.tenant_id)
        .await
        .map_err(ApiError::internal)?;
    if !belongs {
        return Err(ApiError::Forbidden("workspace does not belong to tenant"));
    }

    let case = CaseRepo::new(&state.db)
        .create_case(
            payload.workspace_id,
            payload.chain.clone(),
            payload.network.clone(),
            "created",
        )
        .await
        .map_err(ApiError::internal)?;

    append_audit(
        &state,
        AuditEntry {
            actor_id: tenant.tenant_id,
            action: AuditAction::CaseCreated,
            resource_id: case.id,
            resource_type: "case".to_string(),
            ip_addr: None,
            metadata: serde_json::json!({
                "workspace_id": payload.workspace_id,
                "chain": payload.chain,
                "network": payload.network,
            }),
            occurred_at: None,
        },
    )
    .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreateCaseResponse { id: case.id }),
    ))
}

/// Returns one case together with the latest scan status and checkpoint.
#[tracing::instrument(skip(state))]
pub async fn get_case(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<CaseDetailResponse>, ApiError> {
    let case = CaseRepo::new(&state.db)
        .get_case_with_status(case_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or(ApiError::NotFound("case not found"))?;
    let last_checkpoint = CheckpointRepo::new(&state.db)
        .get_last_checkpoint(case.case.id, case.case.chain.clone(), "height")
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(CaseDetailResponse {
        id: case.case.id,
        workspace_id: case.case.workspace_id,
        chain: case.case.chain,
        network: case.case.network,
        case_status: case.case.status,
        scan_status: case.scan_status,
        created_at: case.case.created_at,
        updated_at: case.case.updated_at,
        last_checkpoint,
    }))
}

/// Lists cases for one workspace together with their latest scan status.
#[tracing::instrument(skip(state))]
pub async fn list_cases_for_workspace(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<CaseSummaryResponse>>, ApiError> {
    let page = query.validate()?;
    let belongs = TenancyRepo::new(&state.db)
        .workspace_belongs_to_tenant(workspace_id, tenant.tenant_id)
        .await
        .map_err(ApiError::internal)?;
    if !belongs {
        return Err(ApiError::Forbidden("workspace does not belong to tenant"));
    }

    let cases = CaseRepo::new(&state.db)
        .list_cases_with_status_for_workspace(workspace_id, page.limit, page.offset)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(PaginatedResponse {
        items: cases
            .into_iter()
            .map(|case| CaseSummaryResponse {
                id: case.case.id,
                workspace_id: case.case.workspace_id,
                chain: case.case.chain,
                network: case.case.network,
                case_status: case.case.status,
                scan_status: case.scan_status,
                created_at: case.case.created_at,
                updated_at: case.case.updated_at,
            })
            .collect(),
        limit: page.limit,
        offset: page.offset,
    }))
}

/// Starts a new scan job for the given case and enqueues it for worker pickup.
#[tracing::instrument(skip(state))]
pub async fn scan_case(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<(axum::http::StatusCode, Json<ScanCaseResponse>), ApiError> {
    let case = CaseRepo::new(&state.db)
        .get_case_by_id(case_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or(ApiError::NotFound("case not found"))?;

    let job = ScanJobRepo::new(&state.db)
        .create_scan_job(
            case.id,
            case.chain.clone(),
            case.network.clone(),
            ScanJobStatus::Created,
        )
        .await
        .map_err(ApiError::internal)?;

    enqueue(
        &state.redis,
        &state.scan_queue_name,
        &ScanJobMessage {
            job_id: job.id,
            case_id: case.id,
            chain: case.chain,
            priority: 5,
        },
    )
    .await
    .map_err(ApiError::internal)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(ScanCaseResponse { job_id: job.id }),
    ))
}

/// Returns the latest scan-job status and last checkpoint for the case.
#[tracing::instrument(skip(state))]
pub async fn get_case_status(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<GetCaseStatusResponse>, ApiError> {
    let case = CaseRepo::new(&state.db)
        .get_case_by_id(case_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or(ApiError::NotFound("case not found"))?;
    let status = ScanJobRepo::new(&state.db)
        .get_latest_scan_job_for_case(case_id)
        .await
        .map_err(ApiError::internal)?
        .map(|job| job.status)
        .unwrap_or(ScanJobStatus::Created);
    let checkpoint = CheckpointRepo::new(&state.db)
        .get_last_checkpoint(case_id, case.chain, "height")
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(GetCaseStatusResponse {
        status,
        last_checkpoint: checkpoint,
    }))
}

/// Lists the canonical events for the case.
#[tracing::instrument(skip(state))]
pub async fn get_case_events(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<Vec<CanonicalEvent>>, ApiError> {
    let events = EventRepo::new(&state.db)
        .get_events_for_case(case_id)
        .await
        .map_err(ApiError::internal)?;
    Ok(Json(events))
}
