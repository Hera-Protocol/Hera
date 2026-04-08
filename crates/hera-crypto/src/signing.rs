use chrono::{DateTime, Utc};
use ed25519_dalek::{Signer, SigningKey};
use serde::{Deserialize, Serialize};

/// Carries the detached signature metadata needed to verify the canonical report
/// artifact independently of any transport or storage layer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ReportSignature {
    /// Names the signature algorithm explicitly so verifiers do not need out-of-band
    /// assumptions about how this artifact was signed.
    pub algorithm: String,
    /// Exposes the verifying public key so any consumer can validate the artifact
    /// without direct access to Hera internals.
    pub public_key_hex: String,
    /// Stores the detached signature in hex so it survives JSON, databases, and
    /// PDF rendering without binary transport issues.
    pub signature_hex: String,
    /// Records when Hera signed the canonical JSON for auditability and later
    /// signature rollover analysis.
    pub signed_at: DateTime<Utc>,
}

/// We sign the canonical JSON, not the PDF — the PDF is derived. If JSON and PDF
/// signatures ever conflict, JSON wins.
pub fn sign_report(signing_key: &SigningKey, canonical_json: &[u8]) -> ReportSignature {
    let verifying_key = signing_key.verifying_key();
    let signature = signing_key.sign(canonical_json);

    ReportSignature {
        algorithm: "ed25519".to_string(),
        public_key_hex: hex::encode(verifying_key.to_bytes()),
        signature_hex: hex::encode(signature.to_bytes()),
        signed_at: Utc::now(),
    }
}
