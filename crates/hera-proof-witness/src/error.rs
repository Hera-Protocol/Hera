use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum WitnessError {
    #[error("invalid amount: {0}")]
    InvalidAmount(String),
    #[error("missing risk score for event {0}")]
    MissingRiskScore(Uuid),
    #[error("invalid txid: {0}")]
    InvalidTxid(String),
    #[error("field overflow: {0}")]
    FieldOverflow(String),
}
