use thiserror::Error;

/// Wraps Postgres failures behind persistence-oriented errors so callers can
/// distinguish connectivity, migration, and mapping issues cleanly.
#[derive(Debug, Error)]
pub enum DbError {
    #[error("database connection failed")]
    Connect(#[source] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
    #[error("database query failed")]
    Query(#[source] sqlx::Error),
    #[error("database returned invalid data: {0}")]
    InvalidData(String),
}
