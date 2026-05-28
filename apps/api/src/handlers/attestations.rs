use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use hera_db::repos::{
    attestation_jobs::AttestationJobRepo, attestations::AttestationRepo, cases::CaseRepo,
    jobs::ScanJobRepo,
};
use hera_types::{
    api::{AttestationDetailResponse, CreateAttestationRequest, CreateAttestationResponse},
    ScanJobStatus,
};
use hera_worker::jobs::attestation_job::{enqueue, AttestationJobMessage};

use crate::{error::ApiError, state::AppState};

const VALID_PROOF_TYPES: &[&str] = &[
    "THRESHOLD_RECEIVED",
    "NO_BLOCKLIST_EXPOSURE",
    "RISK_BELOW_THRESHOLD",
];

#[tracing::instrument(skip(state, body))]
pub async fn create_attestation(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
    Json(body): Json<CreateAttestationRequest>,
) -> Result<(StatusCode, Json<CreateAttestationResponse>), ApiError> {
    if !VALID_PROOF_TYPES.contains(&body.proof_type.as_str()) {
        return Err(ApiError::BadRequest(format!(
            "invalid proof_type: {}",
            body.proof_type
        )));
    }

    // Verify the case exists.
    let _case = CaseRepo::new(&state.db)
        .get_case_by_id(case_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or(ApiError::NotFound("case not found"))?;

    // Verify the case has a completed scan (status = SIGNED).
    let scan = ScanJobRepo::new(&state.db)
        .get_latest_scan_job_for_case(case_id)
        .await
        .map_err(ApiError::internal)?
        .ok_or(ApiError::BadRequest(
            "case must have a completed scan before attestation".into(),
        ))?;

    if scan.status != ScanJobStatus::Signed {
        return Err(ApiError::BadRequest(
            "case scan must be in SIGNED status before attestation".into(),
        ));
    }

    // Create the attestation job.
    let job = AttestationJobRepo::new(&state.db)
        .create(case_id, &body.proof_type, &body.parameters)
        .await
        .map_err(ApiError::internal)?;

    // Enqueue for worker processing.
    enqueue(
        &state.redis,
        &state.attestation_queue_name,
        &AttestationJobMessage {
            job_id: job.id,
            case_id,
            proof_type: body.proof_type,
        },
    )
    .await
    .map_err(ApiError::internal)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateAttestationResponse { job_id: job.id }),
    ))
}

#[tracing::instrument(skip(state))]
pub async fn get_attestation(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
) -> Result<Json<AttestationDetailResponse>, ApiError> {
    let attestations = AttestationRepo::new(&state.db)
        .get_for_case(case_id)
        .await
        .map_err(ApiError::internal)?;

    let attestation = attestations
        .first()
        .ok_or(ApiError::NotFound("no attestation found for case"))?;

    Ok(Json(AttestationDetailResponse {
        id: attestation.id,
        case_id: attestation.case_id,
        proof_type: attestation.proof_type.clone(),
        proof_sha256: attestation.proof_sha256.clone(),
        public_inputs: attestation.public_inputs_json.clone(),
        srs_version: attestation.srs_version.clone(),
        created_at: attestation.created_at,
    }))
}
