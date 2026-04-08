use hera_types::Asset;
use serde::{Deserialize, Serialize};

/// Represents one owned MASP note. `u128` is used because MASP supports
/// high-precision multi-asset amounts. Never use `f64` for on-chain values.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct MaspNote {
    pub txid: String,
    pub block_height: u64,
    pub asset: Asset,
    pub amount_raw: u128,
    pub note_commitment: String,
}

/// Describes how the observed note moved relative to the shielded pool because
/// Namada's MASP flows are not equivalent to Zcash's simpler note categories.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    Shielded,
    Unshielded,
    IntraShielded,
}

/// Records fees as a first-class structure because transparent fee payers are
/// visible and must be recorded as such — do not fold them silently into the shielded event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct FeeRecord {
    pub asset: Asset,
    pub amount_raw: u128,
    pub payer_transparent: Option<String>,
}

/// Persists MASP sync progress using epoch and block cursors because Namada
/// finality and indexing are commonly reasoned about in both units.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct NamadaScanCheckpoint {
    pub last_synced_epoch: u64,
    pub last_synced_block: u64,
}
