use thiserror::Error;

/// Collects Namada-specific failures so MASP and asset-resolution errors stay
/// explicit instead of being flattened into generic transport failures.
#[derive(Debug, Error)]
pub enum NamadaAdapterError {
    #[error("invalid namada viewing key")]
    InvalidViewingKey,
    #[error("masp sync failed: {0}")]
    MaspSyncFailed(String),
    #[error("public masp decoding unavailable: {0}")]
    PublicMaspDecodingUnavailable(String),
    #[error("external masp decoder failed: {0}")]
    ExternalDecoderFailed(String),
    #[error("external masp decoder returned invalid output: {0}")]
    ExternalDecoderProtocol(String),
    #[error("asset resolution failed: {0}")]
    AssetResolutionFailed(String),
    #[error("indexer unavailable: {0}")]
    IndexerUnavailable(String),
    #[error("mapper error: {0}")]
    MapperError(String),
}
