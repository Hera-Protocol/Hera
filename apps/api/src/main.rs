mod config;
mod error;
mod handlers;
mod middleware;
mod router;
mod state;

use std::{net::SocketAddr, sync::Arc};

use deadpool_redis::{Config as RedisConfig, Runtime};
use hera_crypto::{KmsClient, LocalDevKms};
use tracing::info;

use crate::{config::Config, router::build_router, state::AppState};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hera_api=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let db = hera_db::connect(&config.database_url).await?;
    let redis =
        RedisConfig::from_url(config.redis_url.clone()).create_pool(Some(Runtime::Tokio1))?;
    let crypto: Arc<dyn KmsClient> = Arc::new(LocalDevKms::from_env()?);
    let state = AppState {
        db,
        redis,
        crypto,
        kms_key_ref: config.kms_key_ref.clone(),
    };
    let router = build_router(state);
    let addr: SocketAddr = config.bind_addr.parse()?;

    info!(%addr, "starting api server");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}
