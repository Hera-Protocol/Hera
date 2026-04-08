use axum::{
    extract::{Path, State},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use hera_db::repos::{
    cases::CaseRepo, checkpoints::CheckpointRepo, events::EventRepo, jobs::ScanJobRepo,
    tenancy::TenancyRepo,
};
use hera_types::{CanonicalEvent, ChainId, Network, ScanJobStatus};
use hera_worker::jobs::scan_job::{enqueue, ScanJobMessage};

use crate::{
    error::ApiError,
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
            payload.chain,
            payload.network,
            "created",
        )
        .await
        .map_err(ApiError::internal)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(CreateCaseResponse { id: case.id }),
    ))
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateCaseRequest {
    pub workspace_id: Uuid,
    pub chain: ChainId,
    pub network: Network,
}

#[derive(Debug, Serialize)]
pub struct CreateCaseResponse {
    pub id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct ScanCaseResponse {
    pub job_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct GetCaseStatusResponse {
    pub status: ScanJobStatus,
    pub last_checkpoint: Option<u64>,
}
