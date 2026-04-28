use reqwest::StatusCode;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, HeraSdkError>;

#[derive(Debug, Error)]
pub enum HeraSdkError {
    #[error("request failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("hera api returned {status}: {body}")]
    Api { status: StatusCode, body: String },
}
