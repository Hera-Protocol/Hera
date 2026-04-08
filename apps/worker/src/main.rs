mod checkpoint;
mod config;
mod error;
mod jobs;
mod orchestrator;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use deadpool_redis::{Config as RedisConfig, Runtime};
use hera_crypto::{KmsClient, LocalDevKms};
use tokio::sync::watch;
use tracing::{error, info};

use crate::{config::Config, jobs::scan_job::dequeue, orchestrator::ScanOrchestrator};

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "hera_worker=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let db = hera_db::connect(&config.database_url).await?;
    let redis =
        RedisConfig::from_url(config.redis_url.clone()).create_pool(Some(Runtime::Tokio1))?;
    let crypto: Arc<dyn KmsClient> = Arc::new(LocalDevKms::from_env()?);

    // We run multiple worker tasks not multiple processes in Stage 1. Stage 4
    // will scale to separate worker pods.
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let in_flight = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for worker_index in 0..config.worker_concurrency {
        let worker_config = config.clone();
        let worker_db = db.clone();
        let worker_redis = redis.clone();
        let worker_crypto = Arc::clone(&crypto);
        let worker_in_flight = Arc::clone(&in_flight);
        let worker_shutdown = shutdown_rx.clone();

        handles.push(tokio::spawn(async move {
            let orchestrator = ScanOrchestrator {
                db: worker_db,
                redis: worker_redis.clone(),
                crypto: worker_crypto,
                config: worker_config.clone(),
            };
            let mut worker_shutdown = worker_shutdown;

            loop {
                if *worker_shutdown.borrow() {
                    break;
                }

                match dequeue(
                    &worker_redis,
                    &worker_config.queue_name,
                    &worker_config.processing_queue_name,
                )
                .await
                {
                    Ok(Some(message)) => {
                        info!(worker_index, job_id = %message.job_id, "picked up scan job");
                        worker_in_flight.fetch_add(1, Ordering::SeqCst);
                        let result = orchestrator.process_job(message.job_id).await;
                        worker_in_flight.fetch_sub(1, Ordering::SeqCst);

                        if let Err(err) = result {
                            error!(worker_index, job_id = %message.job_id, error = %err, "scan job failed");
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        error!(worker_index, error = %err, "queue poll failed");
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    }
                }
            }
        }));
    }

    wait_for_shutdown_signal().await;
    let _ = shutdown_tx.send(true);
    info!("shutdown signal received, draining in-flight jobs");

    while in_flight.load(Ordering::SeqCst) > 0 {
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }

    for handle in handles {
        let _ = handle.await;
    }

    Ok(())
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        let mut terminate = signal(SignalKind::terminate()).ok();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = async {
                if let Some(signal) = terminate.as_mut() {
                    signal.recv().await;
                }
            } => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
