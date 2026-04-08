use hera_types::Network;
use serde::{Deserialize, Serialize};

use crate::error::ZcashAdapterError;

/// Distinguishes whether the imported key can only detect inbound notes or can
/// also support full wallet-style transaction interpretation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum KeyScope {
    Full,
    Incoming,
}

/// Stores the validated key material alongside scan-critical metadata. Birthday
/// height is critical for scan efficiency. Without it we scan from genesis which
/// is expensive. If not provided, default to a reasonable recent checkpoint but
/// document this assumption in the scan output.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ValidatedZcashKey {
    pub raw_key: String,
    pub key_scope: KeyScope,
    pub birthday_height: Option<u32>,
}

/// Performs conservative string-level validation of UFVK/IVK/FVK material before
/// the worker tries to hand it to lower-level Zcash libraries.
pub fn parse_and_validate(
    raw: &str,
    network: Network,
) -> Result<ValidatedZcashKey, ZcashAdapterError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ZcashAdapterError::InvalidViewingKey(
            "viewing key cannot be empty".into(),
        ));
    }

    // UFVK and FVK encodings are Bech32-like strings. We validate conservatively
    // here instead of guessing full semantics because false acceptance is worse
    // than rejecting a malformed compliance key import.
    let lower = trimmed.to_ascii_lowercase();
    let key_scope = if lower.contains("ivk") || lower.starts_with("zxviewi") {
        KeyScope::Incoming
    } else {
        KeyScope::Full
    };

    let looks_bech32ish = trimmed.contains('1') && trimmed.len() > 16;
    if !looks_bech32ish {
        return Err(ZcashAdapterError::InvalidViewingKey(
            "expected a bech32-like UFVK, IVK, or FVK string".into(),
        ));
    }

    // This network check is intentionally conservative. We only reject strings
    // that self-identify as the opposite environment in their human-readable part.
    let mismatched_network = match network {
        Network::Mainnet => lower.contains("test"),
        Network::Testnet | Network::Regtest => lower.contains("main"),
    };
    if mismatched_network {
        return Err(ZcashAdapterError::InvalidViewingKey(
            "viewing key appears to target a different Zcash network".into(),
        ));
    }

    Ok(ValidatedZcashKey {
        raw_key: trimmed.to_string(),
        key_scope,
        birthday_height: None,
    })
}
