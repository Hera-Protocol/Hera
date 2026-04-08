use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Names the Zcash pool a note belongs to. Transparent pool activity is out of
/// scope for shielded compliance scanning but we model it to avoid silent omission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Pool {
    Orchard,
    Sapling,
    Transparent,
}

/// Encodes memo visibility conservatively. We never pretend a memo is readable
/// if we cannot decrypt it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoVisibility {
    Present { hash: String },
    Absent,
    Encrypted,
}

/// Represents one owned Zcash note discovered during scanning. Amounts stay in
/// zatoshis so the adapter never introduces decimal or float ambiguity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ZcashNote {
    pub txid: String,
    pub block_height: u32,
    pub pool: Pool,
    /// Amounts remain in zatoshis, the smallest unit, because integer arithmetic
    /// avoids the rounding errors that would appear if we used decimal ZEC values here.
    pub amount_zatoshis: u64,
    pub memo: MemoVisibility,
    pub nullifier: Option<String>,
}

/// Persists incremental scan progress so retries can resume from the last known
/// good height instead of rescanning from genesis.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ZcashScanCheckpoint {
    pub last_scanned_height: u32,
    pub scanned_at: DateTime<Utc>,
}
