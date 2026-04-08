use thiserror::Error;

/// Wraps worker failures so queue polling, state transitions, and scan dispatch
/// errors stay explicit and can be mapped into FAILED scan-job states.
#[derive(Debug, Error)]
pub enum OrchestratorError {
    #[error("db error: {0}")]
    Db(#[from] hera_db::DbError),
    #[error("crypto error: {0}")]
    Crypto(#[from] hera_crypto::CryptoError),
    #[error("zcash adapter error: {0}")]
    Zcash(#[from] hera_zcash_adapter::ZcashAdapterError),
    #[error("namada adapter error: {0}")]
    Namada(#[from] hera_namada_adapter::NamadaAdapterError),
    #[error("normalization error: {0}")]
    Normalization(#[from] hera_core::NormalizationError),
    #[error("reporter error: {0}")]
    Reporter(#[from] hera_reporter::ReporterError),
    #[error("queue error: {0}")]
    Queue(String),
    #[error("missing record: {0}")]
    MissingRecord(String),
}
