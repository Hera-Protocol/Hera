use thiserror::Error;

/// Collects Zcash-specific failures so the orchestrator can surface explicit
/// protocol errors instead of collapsing them into generic scan failures.
#[derive(Debug, Error)]
pub enum ZcashAdapterError {
    #[error("invalid zcash viewing key: {0}")]
    InvalidViewingKey(String),
    #[error("unsupported zcash pool: {0}")]
    UnsupportedPool(String),
    #[error("zcash scan failed: {0}")]
    ScanFailed(String),
    #[error("zcash note decryption failed")]
    DecryptionFailed,
    #[error("zcash indexer unavailable: {0}")]
    IndexerUnavailable(String),
    #[error("zcash mapper error: {0}")]
    MapperError(String),
}
