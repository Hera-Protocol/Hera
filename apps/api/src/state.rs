use std::sync::Arc;

use deadpool_redis::Pool as RedisPool;
use hera_crypto::KmsClient;
use hera_db::DbPool;
use hera_reporter::ReportStorage;
use uuid::Uuid;

/// Shared application state passed to handlers and middleware.
#[derive(Clone)]
pub struct AppState {
    pub db: DbPool,
    pub redis: RedisPool,
    pub crypto: Arc<dyn KmsClient>,
    pub report_storage: Arc<ReportStorage>,
    pub kms_key_ref: String,
    pub scan_queue_name: String,
    pub namada_chain_id: String,
}

/// Carries the authenticated tenant identity. Every request carries a tenant
/// context. No endpoint is accessible without a valid API key. This is enforced
/// in middleware, not in each handler.
#[derive(Debug, Clone)]
pub struct TenantContext {
    pub tenant_id: Uuid,
}
