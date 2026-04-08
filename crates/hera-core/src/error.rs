use thiserror::Error;

/// Captures normalization failures at the translation boundary where chain-native
/// data becomes a canonical compliance event.
#[derive(Debug, Error)]
pub enum NormalizationError {
    #[error("invalid amount: {0}")]
    InvalidAmount(String),
    #[error("missing required field: {0}")]
    MissingRequiredField(String),
    #[error("unsupported event type: {0}")]
    UnsupportedEventType(String),
}
