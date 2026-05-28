use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Enumerates the proof types Hera can generate. Each proof type corresponds
/// to a narrow compliance predicate that can be verified without revealing
/// the underlying shielded activity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ProofType {
    ThresholdReceived,
    NoBlocklistExposure,
    RiskBelowThreshold,
}

impl ProofType {
    pub fn from_str_value(s: &str) -> Option<Self> {
        match s {
            "THRESHOLD_RECEIVED" => Some(Self::ThresholdReceived),
            "NO_BLOCKLIST_EXPOSURE" => Some(Self::NoBlocklistExposure),
            "RISK_BELOW_THRESHOLD" => Some(Self::RiskBelowThreshold),
            _ => None,
        }
    }
}

/// The serializable proof artifact stored and served by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AttestationProof {
    pub attestation_version: String,
    pub case_id: uuid::Uuid,
    pub proof_type: ProofType,
    pub proof_bytes_hex: String,
    pub public_inputs_json: serde_json::Value,
    pub srs_version: String,
    pub created_at: DateTime<Utc>,
}

/// Groups the public statement for verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AttestationStatement {
    pub proof_type: ProofType,
    pub parameters: serde_json::Value,
}
