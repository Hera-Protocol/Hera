use ed25519_dalek::SigningKey;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use hera_crypto::{sign_report, ReportSignature};
use hera_types::{CanonicalEvent, Case};

use crate::error::ReporterError;

/// Represents the signed JSON artifact Hera stores and serves as the canonical
/// source of truth for a compliance case.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct SignedManifest {
    pub manifest_version: String,
    pub case_id: uuid::Uuid,
    pub chain: hera_types::ChainId,
    pub network: hera_types::Network,
    pub generated_at: chrono::DateTime<chrono::Utc>,
    pub event_count: usize,
    pub events: Vec<CanonicalEvent>,
    pub signature: ReportSignature,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct UnsignedManifestPayload<'a> {
    manifest_version: &'a str,
    case_id: uuid::Uuid,
    chain: &'a hera_types::ChainId,
    network: &'a hera_types::Network,
    generated_at: chrono::DateTime<chrono::Utc>,
    event_count: usize,
    events: &'a [CanonicalEvent],
}

/// Builds the canonical JSON manifest and signs its compact canonical form. We
/// sign the entire events array serialized to canonical JSON (sorted keys, no
/// extra whitespace) inside a stable manifest payload. This makes the signature
/// reproducible and verifiable without special tooling.
pub fn build_manifest(
    case: &Case,
    events: &[CanonicalEvent],
    signer: &SigningKey,
) -> Result<SignedManifest, ReporterError> {
    let generated_at = chrono::Utc::now();
    let payload = UnsignedManifestPayload {
        manifest_version: "1",
        case_id: case.id,
        chain: &case.chain,
        network: &case.network,
        generated_at,
        event_count: events.len(),
        events,
    };

    // We sign the compact canonical bytes for reproducibility, but store a
    // pretty-printed JSON artifact so humans can inspect it easily.
    let canonical_payload = canonical_json_bytes(&payload)?;
    let signature = sign_report(signer, &canonical_payload);

    Ok(SignedManifest {
        manifest_version: payload.manifest_version.to_string(),
        case_id: case.id,
        chain: case.chain.clone(),
        network: case.network.clone(),
        generated_at,
        event_count: events.len(),
        events: events.to_vec(),
        signature,
    })
}

impl SignedManifest {
    pub fn canonical_payload_bytes(&self) -> Result<Vec<u8>, ReporterError> {
        let payload = UnsignedManifestPayload {
            manifest_version: &self.manifest_version,
            case_id: self.case_id,
            chain: &self.chain,
            network: &self.network,
            generated_at: self.generated_at,
            event_count: self.event_count,
            events: &self.events,
        };
        canonical_json_bytes(&payload)
    }

    pub fn pretty_json_bytes(&self) -> Result<Vec<u8>, ReporterError> {
        serde_json::to_vec_pretty(self)
            .map_err(|err| ReporterError::ManifestSerialization(err.to_string()))
    }
}

fn canonical_json_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, ReporterError> {
    let json_value = serde_json::to_value(value)
        .map_err(|err| ReporterError::ManifestSerialization(err.to_string()))?;
    let canonical = sort_json_value(json_value);
    serde_json::to_vec(&canonical)
        .map_err(|err| ReporterError::ManifestSerialization(err.to_string()))
}

fn sort_json_value(value: Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut entries: Vec<_> = map.into_iter().collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            entries
                .into_iter()
                .map(|(key, value)| (key, sort_json_value(value)))
                .collect()
        }
        Value::Array(items) => Value::Array(items.into_iter().map(sort_json_value).collect()),
        other => other,
    }
}
