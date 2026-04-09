use std::{net::SocketAddr, sync::Arc};

use deadpool_redis::{Config as RedisConfig, Runtime};
use hera_api::{config::Config, router::build_router, state::AppState};
use hera_crypto::{KmsClient, LocalDevKms};
use hera_reporter::{build_s3_client, ReportStorage};
use tracing::info;

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
    let report_storage = Arc::new(ReportStorage::new(
        build_s3_client(&config.aws_region, config.aws_endpoint_url.as_deref()).await,
        config.report_bucket.clone(),
        db.clone(),
        None,
    ));
    let state = AppState {
        db,
        redis,
        crypto,
        report_storage,
        kms_key_ref: config.kms_key_ref.clone(),
        scan_queue_name: config.scan_queue_name.clone(),
        namada_chain_id: config.namada_chain_id.clone(),
    };
    let router = build_router(state);
    let addr: SocketAddr = config.bind_addr.parse()?;

    info!(%addr, "starting api server");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}
