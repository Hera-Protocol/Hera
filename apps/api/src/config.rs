use std::env;

use thiserror::Error;

/// Holds the API runtime configuration so external dependencies are explicit at
/// startup instead of being discovered lazily during request handling.
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub bind_addr: String,
    pub kms_key_ref: String,
    pub scan_queue_name: String,
    pub attestation_queue_name: String,
    pub namada_chain_id: String,
    pub aws_region: String,
    pub aws_endpoint_url: Option<String>,
    pub report_bucket: String,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            redis_url: required("REDIS_URL")?,
            bind_addr: optional("API_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:3000".to_string()),
            kms_key_ref: required("KMS_KEY_ID")?,
            scan_queue_name: optional("SCAN_QUEUE_NAME")
                .unwrap_or_else(|| "hera:scan:pending".to_string()),
            attestation_queue_name: optional("ATTESTATION_QUEUE_NAME")
                .unwrap_or_else(|| "hera:attestation:pending".to_string()),
            namada_chain_id: required("NAMADA_CHAIN_ID")?,
            aws_region: required("AWS_REGION")?,
            aws_endpoint_url: optional("AWS_ENDPOINT_URL"),
            report_bucket: required("S3_BUCKET")?,
        })
    }
}

fn required(name: &str) -> Result<String, ConfigError> {
    env::var(name).map_err(|_| ConfigError::MissingEnv(name.to_string()))
}

fn optional(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

/// Reports invalid API configuration before the server starts listening.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing environment variable: {0}")]
    MissingEnv(String),
}
