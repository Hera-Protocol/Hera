use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ApiError, state::AppState};

#[derive(Debug, Deserialize)]
pub struct DemoModeQuery {
    pub mode: Option<String>,
}

/// Combined demo response that includes standard report data and optionally
/// ZK attestation data when `?mode=zk` is passed.
#[derive(Debug, Serialize)]
pub struct DemoReportResponse {
    pub case_id: String,
    pub mode: String,
    pub report: DemoStandardReport,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation: Option<DemoZkAttestation>,
}

#[derive(Debug, Serialize)]
pub struct DemoStandardReport {
    pub manifest_version: String,
    pub chain: String,
    pub network: String,
    pub generated_at: String,
    pub total_events: usize,
    pub summary: DemoSummary,
    pub events: Vec<DemoEvent>,
    pub signature: DemoSignature,
}

#[derive(Debug, Serialize)]
pub struct DemoSummary {
    pub audit_start: String,
    pub audit_end: String,
    pub total_received: String,
    pub total_sent: String,
    pub distinct_assets: Vec<String>,
    pub event_type_counts: DemoEventTypeCounts,
}

#[derive(Debug, Serialize)]
pub struct DemoEventTypeCounts {
    pub shield: usize,
    pub receive: usize,
    pub send: usize,
    pub unshield: usize,
    pub fee: usize,
}

#[derive(Debug, Serialize)]
pub struct DemoEvent {
    pub event_id: String,
    pub event_type: String,
    pub txid: String,
    pub block_height: u64,
    pub timestamp: String,
    pub asset: DemoAsset,
    pub amount: String,
    pub counterparty: DemoCounterparty,
    pub memo: DemoMemo,
    pub provenance: DemoProvenance,
}

#[derive(Debug, Serialize)]
pub struct DemoAsset {
    pub symbol: String,
    pub asset_id: String,
    pub decimals: u8,
}

#[derive(Debug, Serialize)]
pub struct DemoCounterparty {
    pub visibility: String,
    pub value: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DemoMemo {
    pub present: bool,
    pub hash: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DemoProvenance {
    pub source: String,
    pub pool: Option<String>,
    pub scan_version: String,
}

#[derive(Debug, Serialize)]
pub struct DemoSignature {
    pub algorithm: String,
    pub public_key_hex: String,
    pub signature_hex: String,
    pub signed_at: String,
}

#[derive(Debug, Serialize)]
pub struct DemoZkAttestation {
    pub attestation_version: String,
    pub proof_type: String,
    pub srs_version: String,
    pub public_inputs: serde_json::Value,
    pub proof_sha256: String,
    pub verified: bool,
    pub created_at: String,
}

fn build_demo_events() -> Vec<DemoEvent> {
    vec![
        DemoEvent {
            event_id: "a1b2c3d4-e5f6-7890-abcd-ef1234567890".into(),
            event_type: "RECEIVE".into(),
            txid: "f4184fc596403b9d638783cf57adfe4c75c605f6356fbc91338530e9831e9e16".into(),
            block_height: 2_100_000,
            timestamp: "2026-05-20T10:30:00Z".into(),
            asset: DemoAsset {
                symbol: "ZEC".into(),
                asset_id: "zcash".into(),
                decimals: 8,
            },
            amount: "1.50000000".into(),
            counterparty: DemoCounterparty {
                visibility: "UNKNOWN".into(),
                value: None,
            },
            memo: DemoMemo {
                present: true,
                hash: Some("a3f2b1...".into()),
            },
            provenance: DemoProvenance {
                source: "lightwalletd".into(),
                pool: Some("sapling".into()),
                scan_version: "stage2".into(),
            },
        },
        DemoEvent {
            event_id: "b2c3d4e5-f6a7-8901-bcde-f12345678901".into(),
            event_type: "RECEIVE".into(),
            txid: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".into(),
            block_height: 2_100_050,
            timestamp: "2026-05-21T14:15:00Z".into(),
            asset: DemoAsset {
                symbol: "ZEC".into(),
                asset_id: "zcash".into(),
                decimals: 8,
            },
            amount: "3.25000000".into(),
            counterparty: DemoCounterparty {
                visibility: "UNKNOWN".into(),
                value: None,
            },
            memo: DemoMemo {
                present: false,
                hash: None,
            },
            provenance: DemoProvenance {
                source: "lightwalletd".into(),
                pool: Some("sapling".into()),
                scan_version: "stage2".into(),
            },
        },
        DemoEvent {
            event_id: "c3d4e5f6-a7b8-9012-cdef-123456789012".into(),
            event_type: "SEND".into(),
            txid: "d4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35".into(),
            block_height: 2_100_100,
            timestamp: "2026-05-22T09:45:00Z".into(),
            asset: DemoAsset {
                symbol: "ZEC".into(),
                asset_id: "zcash".into(),
                decimals: 8,
            },
            amount: "0.75000000".into(),
            counterparty: DemoCounterparty {
                visibility: "PARTIAL".into(),
                value: Some("t1Rv4exT...".into()),
            },
            memo: DemoMemo {
                present: false,
                hash: None,
            },
            provenance: DemoProvenance {
                source: "lightwalletd".into(),
                pool: Some("sapling".into()),
                scan_version: "stage2".into(),
            },
        },
        DemoEvent {
            event_id: "d4e5f6a7-b8c9-0123-defa-234567890123".into(),
            event_type: "FEE".into(),
            txid: "d4735e3a265e16eee03f59718b9b5d03019c07d8b6c51f90da3a666eec13ab35".into(),
            block_height: 2_100_100,
            timestamp: "2026-05-22T09:45:00Z".into(),
            asset: DemoAsset {
                symbol: "ZEC".into(),
                asset_id: "zcash".into(),
                decimals: 8,
            },
            amount: "0.00010000".into(),
            counterparty: DemoCounterparty {
                visibility: "UNKNOWN".into(),
                value: None,
            },
            memo: DemoMemo {
                present: false,
                hash: None,
            },
            provenance: DemoProvenance {
                source: "lightwalletd".into(),
                pool: Some("sapling".into()),
                scan_version: "stage2".into(),
            },
        },
    ]
}

fn build_standard_report(_case_id: &str) -> DemoStandardReport {
    DemoStandardReport {
        manifest_version: "1.0.0".into(),
        chain: "ZCASH".into(),
        network: "MAINNET".into(),
        generated_at: "2026-05-28T12:00:00Z".into(),
        total_events: 4,
        summary: DemoSummary {
            audit_start: "2026-05-20T10:30:00Z".into(),
            audit_end: "2026-05-22T09:45:00Z".into(),
            total_received: "4.75000000".into(),
            total_sent: "0.75000000".into(),
            distinct_assets: vec!["ZEC".into()],
            event_type_counts: DemoEventTypeCounts {
                shield: 0,
                receive: 2,
                send: 1,
                unshield: 0,
                fee: 1,
            },
        },
        events: build_demo_events(),
        signature: DemoSignature {
            algorithm: "ed25519".into(),
            public_key_hex: "d75a980182b10ab7d54bfed3c964073a0ee172f3daa3f4a18446b7e8b47a18e7".into(),
            signature_hex: "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e065224901555fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b".into(),
            signed_at: "2026-05-28T12:00:00Z".into(),
        },
    }
}

fn build_zk_attestation() -> DemoZkAttestation {
    DemoZkAttestation {
        attestation_version: "1.0.0".into(),
        proof_type: "THRESHOLD_RECEIVED".into(),
        srs_version: "caulk-plus-v1".into(),
        public_inputs: serde_json::json!({
            "proof_type": "THRESHOLD_RECEIVED",
            "threshold": 100000000,
            "event_count": 4,
            "verified": true
        }),
        proof_sha256: "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9".into(),
        verified: true,
        created_at: "2026-05-28T12:01:00Z".into(),
    }
}

/// `GET /v1/demo/cases/:id/report` — returns demo mock data.
/// Pass `?mode=standard` for Stage 2 report only, `?mode=zk` for Stage 2 + Stage 3 attestation.
#[tracing::instrument(skip(state))]
pub async fn demo_report(
    State(state): State<AppState>,
    Path(case_id): Path<Uuid>,
    Query(query): Query<DemoModeQuery>,
) -> Result<Json<DemoReportResponse>, ApiError> {
    if !state.demo_mode {
        return Err(ApiError::NotFound("demo mode is not enabled"));
    }

    let mode = query.mode.unwrap_or_else(|| "standard".into());
    let case_id_str = case_id.to_string();

    let attestation = if mode == "zk" {
        Some(build_zk_attestation())
    } else {
        None
    };

    Ok(Json(DemoReportResponse {
        case_id: case_id_str.clone(),
        mode: mode.clone(),
        report: build_standard_report(&case_id_str),
        attestation,
    }))
}

/// `GET /v1/demo/attestation-types` — lists available proof types for the frontend.
#[tracing::instrument(skip(state))]
pub async fn demo_attestation_types(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    if !state.demo_mode {
        return Err(ApiError::NotFound("demo mode is not enabled"));
    }

    Ok(Json(serde_json::json!({
        "proof_types": [
            {
                "id": "THRESHOLD_RECEIVED",
                "name": "Total Received Above Threshold",
                "description": "Proves that the total received amount across all shielded transactions exceeds a given threshold, without revealing individual amounts.",
                "parameters": {
                    "threshold": {
                        "type": "integer",
                        "description": "Threshold in smallest units (e.g., zatoshi)"
                    }
                }
            },
            {
                "id": "NO_BLOCKLIST_EXPOSURE",
                "name": "No Blocklist Exposure",
                "description": "Proves that none of the user's transaction IDs appear in a provided blocklist of sanctioned/flagged transactions.",
                "parameters": {
                    "blocklist": {
                        "type": "array",
                        "description": "Array of blocklisted transaction hash values"
                    }
                }
            },
            {
                "id": "RISK_BELOW_THRESHOLD",
                "name": "Risk Score Below Threshold",
                "description": "Proves that the maximum risk score across all events is at or below a given threshold (0-100).",
                "parameters": {
                    "max_risk": {
                        "type": "integer",
                        "description": "Maximum acceptable risk score (0-100)"
                    }
                }
            }
        ]
    })))
}
