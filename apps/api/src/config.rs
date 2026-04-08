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
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            database_url: required("DATABASE_URL")?,
            redis_url: required("REDIS_URL")?,
            bind_addr: optional("API_BIND_ADDR").unwrap_or_else(|| "0.0.0.0:3000".to_string()),
            kms_key_ref: required("KMS_KEY_ID")?,
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
