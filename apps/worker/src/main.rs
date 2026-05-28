mod attestation_orchestrator;
mod checkpoint;
mod config;
mod error;
mod jobs;
mod orchestrator;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use deadpool_redis::{Config as RedisConfig, Runtime};
use ed25519_dalek::SigningKey;
use hera_crypto::{KmsClient, LocalDevKms};
use hera_proof_circuits::caulk::srs::CaulkPlusSrs;
use hera_reporter::{build_s3_client, AttestationStorage, ReportStorage};
use tokio::sync::watch;
use tracing::{error, info};

use crate::{
    attestation_orchestrator::AttestationOrchestrator,
    config::Config,
    jobs::{attestation_job::dequeue as dequeue_attestation, scan_job::dequeue},
    orchestrator::ScanOrchestrator,
};

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
    let signing_key = Arc::new(load_signing_key_from_env()?);
    let report_storage = Arc::new(ReportStorage::new(
        build_s3_client(&config.aws_region, config.aws_endpoint_url.as_deref()).await,
        config.report_bucket.clone(),
        db.clone(),
        config.report_kms_key_id.clone(),
    ));
    if config.aws_endpoint_url.is_some() {
        report_storage.ensure_bucket().await?;
    }

    let attestation_storage = Arc::new(AttestationStorage::new(
        build_s3_client(&config.aws_region, config.aws_endpoint_url.as_deref()).await,
        config.report_bucket.clone(),
        db.clone(),
        config.report_kms_key_id.clone(),
    ));

    info!("generating dev SRS for Caulk+ proofs");
    let srs = Arc::new(CaulkPlusSrs::dev_srs().expect("failed to generate dev SRS"));
    info!("SRS ready (max_degree={})", srs.max_degree);

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
        let worker_report_storage = Arc::clone(&report_storage);
        let worker_signing_key = Arc::clone(&signing_key);
        let worker_in_flight = Arc::clone(&in_flight);
        let worker_shutdown = shutdown_rx.clone();

        handles.push(tokio::spawn(async move {
            let orchestrator = ScanOrchestrator {
                db: worker_db,
                _redis: worker_redis.clone(),
                crypto: worker_crypto,
                report_storage: worker_report_storage,
                report_signing_key: worker_signing_key,
                config: worker_config.clone(),
            };
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

    // Attestation worker loop — single-threaded for now (proof generation is CPU-bound).
    {
        let attest_config = config.clone();
        let attest_db = db.clone();
        let attest_redis = redis.clone();
        let attest_storage = Arc::clone(&attestation_storage);
        let attest_srs = Arc::clone(&srs);
        let attest_in_flight = Arc::clone(&in_flight);
        let attest_shutdown = shutdown_rx.clone();

        handles.push(tokio::spawn(async move {
            let orchestrator = AttestationOrchestrator {
                db: attest_db,
                _redis: attest_redis.clone(),
                attestation_storage: attest_storage,
                srs: attest_srs,
                config: attest_config.clone(),
            };
            loop {
                if *attest_shutdown.borrow() {
                    break;
                }

                match dequeue_attestation(
                    &attest_redis,
                    &attest_config.attestation_queue_name,
                    &attest_config.attestation_processing_queue_name,
                )
                .await
                {
                    Ok(Some(message)) => {
                        info!(job_id = %message.job_id, "picked up attestation job");
                        attest_in_flight.fetch_add(1, Ordering::SeqCst);
                        let result = orchestrator.process_job(message.job_id).await;
                        attest_in_flight.fetch_sub(1, Ordering::SeqCst);

                        if let Err(err) = result {
                            error!(job_id = %message.job_id, error = %err, "attestation job failed");
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        error!(error = %err, "attestation queue poll failed");
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

fn load_signing_key_from_env() -> anyhow::Result<SigningKey> {
    let seed_b64 = std::env::var("SIGNING_KEY_BASE64")
        .map_err(|_| anyhow::anyhow!("missing SIGNING_KEY_BASE64"))?;
    let seed = BASE64_STANDARD
        .decode(seed_b64.trim())
        .map_err(|err| anyhow::anyhow!("invalid SIGNING_KEY_BASE64: {err}"))?;
    let seed: [u8; 32] = seed
        .try_into()
        .map_err(|_| anyhow::anyhow!("SIGNING_KEY_BASE64 must decode to exactly 32 bytes"))?;
    Ok(SigningKey::from_bytes(&seed))
}
