use std::sync::Arc;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use hera_crypto::encrypt_viewing_key;
use hera_db::repos::keys::ViewKeyRepo;
use hera_types::ChainId;

use crate::{error::ApiError, state::AppState};

/// Accepts a Zcash viewing key, encrypts it immediately, and persists only the
/// ciphertext. The raw key is encrypted and zeroized before any DB write. It
/// must not appear in logs, traces, or error messages.
#[tracing::instrument(skip(state, payload))]
pub async fn import_zcash_view_key(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
    Json(payload): Json<ImportViewingKeyRequest>,
) -> Result<(axum::http::StatusCode, Json<ImportViewingKeyResponse>), ApiError> {
    import_view_key(state, case_id, ChainId::Zcash, payload).await
}

/// Accepts a Namada viewing key, encrypts it immediately, and persists only the
/// ciphertext. The raw key is encrypted and zeroized before any DB write. It
/// must not appear in logs, traces, or error messages.
#[tracing::instrument(skip(state, payload))]
pub async fn import_namada_view_key(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
    Json(payload): Json<ImportViewingKeyRequest>,
) -> Result<(axum::http::StatusCode, Json<ImportViewingKeyResponse>), ApiError> {
    import_view_key(state, case_id, ChainId::Namada, payload).await
}

async fn import_view_key(
    state: AppState,
    case_id: Uuid,
    chain: ChainId,
    payload: ImportViewingKeyRequest,
) -> Result<(axum::http::StatusCode, Json<ImportViewingKeyResponse>), ApiError> {
    if payload.raw_key.trim().is_empty() {
        return Err(ApiError::BadRequest("raw_key is required".into()));
    }

    let logical_key_ref = Uuid::new_v4();
    let raw_key = Zeroizing::new(payload.raw_key);
    let encrypted = encrypt_viewing_key(
        Arc::as_ref(&state.crypto),
        format!("{}:{logical_key_ref}", state.kms_key_ref),
        raw_key.as_bytes().to_vec(),
    )
    .await
    .map_err(ApiError::internal)?;

    ViewKeyRepo::new(&state.db)
        .store_encrypted_view_key(
            case_id,
            chain,
            &encrypted.key_ref,
            &encrypted.ciphertext,
            &encrypted.nonce,
            &encrypted.encrypted_data_key,
            payload.birthday_height,
        )
        .await
        .map_err(ApiError::internal)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(ImportViewingKeyResponse {
            key_ref: logical_key_ref,
        }),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportViewingKeyRequest {
    pub raw_key: String,
    pub birthday_height: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct ImportViewingKeyResponse {
    pub key_ref: Uuid,
}
