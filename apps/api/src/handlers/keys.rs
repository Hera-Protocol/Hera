use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use hera_crypto::encrypt_viewing_key;
use hera_db::repos::{
    audit::{AuditAction, AuditEntry},
    keys::{EncryptedViewKeyRecord, ViewKeyRepo},
    tenancy::TenancyRepo,
};
use hera_namada_adapter::parse_and_validate as parse_namada_view_key;
use hera_types::ChainId;

use crate::{
    error::ApiError,
    handlers::{append_audit, PaginatedResponse, PaginationQuery},
    state::{AppState, TenantContext},
};

/// Accepts a Zcash viewing key, encrypts it immediately, and persists only the
/// ciphertext. The raw key is encrypted and zeroized before any DB write. It
/// must not appear in logs, traces, or error messages.
#[tracing::instrument(skip(state, payload))]
pub async fn import_zcash_view_key(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Path(case_id): Path<Uuid>,
    Json(payload): Json<ImportViewingKeyRequest>,
) -> Result<(axum::http::StatusCode, Json<ImportViewingKeyResponse>), ApiError> {
    import_view_key(state, tenant.tenant_id, case_id, ChainId::Zcash, payload).await
}

/// Accepts a Namada viewing key, encrypts it immediately, and persists only the
/// ciphertext. The raw key is encrypted and zeroized before any DB write. It
/// must not appear in logs, traces, or error messages.
#[tracing::instrument(skip(state, payload))]
pub async fn import_namada_view_key(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Path(case_id): Path<Uuid>,
    Json(payload): Json<ImportViewingKeyRequest>,
) -> Result<(axum::http::StatusCode, Json<ImportViewingKeyResponse>), ApiError> {
    import_view_key(state, tenant.tenant_id, case_id, ChainId::Namada, payload).await
}

async fn import_view_key(
    state: AppState,
    tenant_id: Uuid,
    case_id: Uuid,
    chain: ChainId,
    payload: ImportViewingKeyRequest,
) -> Result<(axum::http::StatusCode, Json<ImportViewingKeyResponse>), ApiError> {
    if payload.raw_key.trim().is_empty() {
        return Err(ApiError::BadRequest("raw_key is required".into()));
    }

    let mut birthday_height = payload.birthday_height;
    let canonical_raw_key = match chain {
        ChainId::Namada => {
            let validated = parse_namada_view_key(&payload.raw_key, &state.namada_chain_id)
                .map_err(|_| ApiError::BadRequest("invalid Namada viewing key".into()))?;
            if let (Some(request_birthday), Some(key_birthday)) =
                (birthday_height, validated.birthday_height)
            {
                if request_birthday != key_birthday {
                    return Err(ApiError::BadRequest(
                        "birthday_height conflicts with the viewing key suffix".into(),
                    ));
                }
            }
            birthday_height = birthday_height.or(validated.birthday_height);
            validated.raw_key
        }
        ChainId::Zcash => payload.raw_key,
    };

    let logical_key_ref = Uuid::new_v4();
    let raw_key = Zeroizing::new(canonical_raw_key);
    let encrypted = encrypt_viewing_key(
        Arc::as_ref(&state.crypto),
        format!("{}:{logical_key_ref}", state.kms_key_ref),
        raw_key.as_bytes().to_vec(),
    )
    .await
    .map_err(ApiError::internal)?;

    ViewKeyRepo::new(&state.db)
        .store_encrypted_view_key(EncryptedViewKeyRecord {
            case_id,
            chain: chain.clone(),
            key_ref: &encrypted.key_ref,
            ciphertext: &encrypted.ciphertext,
            nonce: &encrypted.nonce,
            encrypted_data_key: &encrypted.encrypted_data_key,
            birthday_height,
        })
        .await
        .map_err(ApiError::internal)?;

    append_audit(
        &state,
        AuditEntry {
            actor_id: tenant_id,
            action: AuditAction::KeyImport,
            resource_id: case_id,
            resource_type: "case".to_string(),
            ip_addr: None,
            metadata: serde_json::json!({
                "case_id": case_id,
                "chain": chain,
                "birthday_height": birthday_height,
                "key_ref": logical_key_ref,
            }),
            occurred_at: None,
        },
    )
    .await?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(ImportViewingKeyResponse {
            key_ref: logical_key_ref,
        }),
    ))
}

/// Lists viewing keys imported for one workspace.
#[tracing::instrument(skip(state))]
pub async fn list_workspace_view_keys(
    State(state): State<AppState>,
    Extension(tenant): Extension<TenantContext>,
    Path(workspace_id): Path<Uuid>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<PaginatedResponse<WorkspaceViewKeyResponse>>, ApiError> {
    let page = query.validate()?;
    let belongs = TenancyRepo::new(&state.db)
        .workspace_belongs_to_tenant(workspace_id, tenant.tenant_id)
        .await
        .map_err(ApiError::internal)?;
    if !belongs {
        return Err(ApiError::Forbidden("workspace does not belong to tenant"));
    }

    let keys = ViewKeyRepo::new(&state.db)
        .list_view_keys_for_workspace(workspace_id, page.limit, page.offset)
        .await
        .map_err(ApiError::internal)?;

    Ok(Json(PaginatedResponse {
        items: keys
            .into_iter()
            .map(|key| WorkspaceViewKeyResponse {
                id: key.id,
                case_id: key.case_id,
                chain: key.chain,
                key_ref: key.key_ref,
                birthday_height: key.birthday_height,
                created_at: key.created_at,
                updated_at: key.updated_at,
            })
            .collect(),
        limit: page.limit,
        offset: page.offset,
    }))
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

#[derive(Debug, Serialize)]
pub struct WorkspaceViewKeyResponse {
    pub id: Uuid,
    pub case_id: Uuid,
    pub chain: ChainId,
    pub key_ref: String,
    pub birthday_height: Option<u64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
