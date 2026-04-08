mod config;
mod error;
mod handlers;
mod middleware;
mod router;
mod state;

use std::{net::SocketAddr, sync::Arc};

use aws_config::{BehaviorVersion, Region};
use deadpool_redis::{Config as RedisConfig, Runtime};
use hera_crypto::{KmsClient, LocalDevKms};
use hera_reporter::ReportStorage;
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
    let report_storage = Arc::new(ReportStorage::new(
        build_s3_client(&config).await,
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
    };
    let router = build_router(state);
    let addr: SocketAddr = config.bind_addr.parse()?;

    info!(%addr, "starting api server");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, router).await?;
    Ok(())
}

async fn build_s3_client(config: &Config) -> aws_sdk_s3::Client {
    let shared_config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(config.aws_region.clone()))
        .load()
        .await;

    let mut builder = aws_sdk_s3::config::Builder::from(&shared_config);
    if let Some(endpoint_url) = &config.aws_endpoint_url {
        builder = builder.endpoint_url(endpoint_url);
    }

    aws_sdk_s3::Client::from_conf(builder.build())
}
