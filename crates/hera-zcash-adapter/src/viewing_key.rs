use hera_types::Network;
use serde::{Deserialize, Serialize};
use zcash_keys::keys::{UnifiedFullViewingKey, UnifiedIncomingViewingKey};
use zcash_protocol::consensus::{MAIN_NETWORK, TEST_NETWORK};

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

/// Validates the serialized viewing key with the canonical Zcash key parsers so
/// later scanning code can rely on network-correct UFVK/UIVK semantics instead
/// of hand-rolled string heuristics.
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

    let key_scope = match network {
        Network::Mainnet => decode_scope(trimmed, &MAIN_NETWORK)?,
        // Regtest uses testnet-style encodings for unified viewing keys, so we
        // validate against testnet parameters and keep the explicit Regtest
        // selection for the network context carried above the adapter.
        Network::Testnet | Network::Regtest => decode_scope(trimmed, &TEST_NETWORK)?,
    };

    Ok(ValidatedZcashKey {
        raw_key: trimmed.to_string(),
        key_scope,
        birthday_height: None,
    })
}

fn decode_scope<P>(raw: &str, params: &P) -> Result<KeyScope, ZcashAdapterError>
where
    P: zcash_protocol::consensus::Parameters,
{
    if UnifiedFullViewingKey::decode(params, raw).is_ok() {
        return Ok(KeyScope::Full);
    }

    if UnifiedIncomingViewingKey::decode(params, raw).is_ok() {
        return Ok(KeyScope::Incoming);
    }

    Err(ZcashAdapterError::InvalidViewingKey(
        "expected a valid UFVK or UIVK for the requested network".into(),
    ))
}
