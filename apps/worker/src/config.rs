use std::env;

use thiserror::Error;

/// Holds the worker runtime configuration so startup is explicit about every
/// external dependency and concurrency choice.
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub redis_url: String,
    pub worker_concurrency: usize,
    pub queue_name: String,
    pub processing_queue_name: String,
    pub lightwalletd_url: String,
    pub namada_indexer_url: String,
    pub scan_engine_version: String,
    pub aws_region: String,
    pub aws_endpoint_url: Option<String>,
    pub report_bucket: String,
    pub report_kms_key_id: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            redis_url: required("REDIS_URL")?,
            worker_concurrency: optional("WORKER_CONCURRENCY")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(4),
            queue_name: optional("SCAN_QUEUE_NAME")
                .unwrap_or_else(|| "hera:scan:pending".to_string()),
            processing_queue_name: optional("SCAN_PROCESSING_QUEUE_NAME")
                .unwrap_or_else(|| "hera:scan:processing".to_string()),
            lightwalletd_url: required("LIGHTWALLETD_URL")?,
            namada_indexer_url: required("NAMADA_INDEXER_URL")?,
            scan_engine_version: optional("SCAN_ENGINE_VERSION")
                .unwrap_or_else(|| "stage1".to_string()),
            aws_region: required("AWS_REGION")?,
            aws_endpoint_url: optional("AWS_ENDPOINT_URL"),
            report_bucket: required("S3_BUCKET")?,
            report_kms_key_id: optional("KMS_KEY_ID"),
        })
    }
}

fn required(name: &str) -> Result<String, ConfigError> {
    env::var(name).map_err(|_| ConfigError::MissingEnv(name.to_string()))
}

fn optional(name: &str) -> Option<String> {
    env::var(name).ok().filter(|value| !value.trim().is_empty())
}

/// Reports invalid worker configuration before startup side effects begin.
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("missing environment variable: {0}")]
    MissingEnv(String),
}
