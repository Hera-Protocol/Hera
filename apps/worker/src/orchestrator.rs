mod chain;
mod context;

use std::{future::Future, sync::Arc, time::Duration};

use deadpool_redis::Pool as RedisPool;
use ed25519_dalek::SigningKey;
use hera_crypto::{decrypt_viewing_key, EncryptedViewingKey, KmsClient};
use hera_db::{
    repos::{
        audit::{AuditAction, AuditEntry, AuditRepo},
        jobs::ScanJobRepo,
        keys::StoredViewingKey,
    },
    DbPool,
};
use hera_reporter::ReportStorage;
use hera_types::ScanJobStatus;
use rand::{thread_rng, Rng};
use tracing::info;
use uuid::Uuid;

use crate::{config::Config, error::OrchestratorError};

/// Drives the durable scan-job state machine for one queued job.
pub struct ScanOrchestrator {
    pub db: DbPool,
    pub _redis: RedisPool,
    pub crypto: Arc<dyn KmsClient>,
    pub report_storage: Arc<ReportStorage>,
    pub report_signing_key: Arc<SigningKey>,
    pub config: Config,
}

impl ScanOrchestrator {
    /// Processes one scan job end to end. Each state transition updates
    /// `scan_jobs`, appends an audit entry, and stops on the first failure.
    pub async fn process_job(&self, job_id: Uuid) -> Result<(), OrchestratorError> {
        let loaded = self.load_job_context(job_id).await?;

        let outcome: Result<(), OrchestratorError> = async {
            // KEY_VALIDATED decrypts the stored ciphertext and validates the key
            // before any network scan begins.
            self.transition(loaded.job.id, ScanJobStatus::KeyValidated)
                .await?;
            self.append_audit(loaded.job.id, AuditAction::KeyRead)
                .await?;
            let validated_key = self.validate_chain_key(&loaded).await?;

            // CHAIN_SYNCING contacts the chain-specific remote and persists
            // progress so interrupted scans resume from the latest checkpoint.
            self.transition(loaded.job.id, ScanJobStatus::ChainSyncing)
                .await?;
            self.append_audit(loaded.job.id, AuditAction::ScanStarted)
                .await?;
            self.run_chain_scan(&loaded, validated_key).await?;

            // BUILDING_REPORT is explicit because artifact generation can fail
            // independently of scanning and normalization.
            self.transition(loaded.job.id, ScanJobStatus::BuildingReport)
                .await?;
            self.finalize_report(&loaded.case).await?;
            self.append_audit(loaded.job.id, AuditAction::ReportExported)
                .await?;

            // SIGNED records successful artifact completion. Prompt 9 will wire
            // the stored artifacts and detached signature behind a final durable
            // job-state transition.
            self.transition(loaded.job.id, ScanJobStatus::Signed)
                .await?;
            self.append_audit(loaded.job.id, AuditAction::StatusChanged)
                .await?;
            Ok(())
        }
        .await;

        if let Err(err) = outcome {
            let reason = err.to_string();
            let _ = self
                .transition(loaded.job.id, ScanJobStatus::Failed(reason))
                .await;
            let _ = self
                .append_audit(loaded.job.id, AuditAction::StatusChanged)
                .await;
            return Err(err);
        }

        Ok(())
    }

    async fn transition(
        &self,
        job_id: Uuid,
        status: ScanJobStatus,
    ) -> Result<(), OrchestratorError> {
        info!(%job_id, status = ?status, "scan job transition start");
        ScanJobRepo::new(&self.db)
            .update_scan_job_status(job_id, status.clone())
            .await?
            .ok_or_else(|| OrchestratorError::MissingRecord(format!("scan job {job_id}")))?;
        info!(%job_id, status = ?status, "scan job transition complete");
        Ok(())
    }

    async fn append_audit(
        &self,
        job_id: Uuid,
        action: AuditAction,
    ) -> Result<(), OrchestratorError> {
        AuditRepo::new(&self.db)
            .append_audit_log(AuditEntry {
                actor_id: job_id,
                action,
                resource_id: job_id,
                resource_type: "scan_job".to_string(),
                ip_addr: None,
                metadata: serde_json::json!({}),
                occurred_at: None,
            })
            .await?;
        Ok(())
    }

    async fn decrypt_view_key(
        &self,
        stored_key: &StoredViewingKey,
    ) -> Result<Vec<u8>, OrchestratorError> {
        let encrypted = EncryptedViewingKey {
            ciphertext: stored_key.ciphertext.clone(),
            nonce: stored_key.nonce.clone(),
            key_ref: stored_key.key_ref.clone(),
            encrypted_data_key: stored_key.encrypted_data_key.clone(),
        };
        decrypt_viewing_key(self.crypto.as_ref(), &encrypted)
            .await
            .map_err(OrchestratorError::from)
    }

    async fn with_retry<T, F, Fut>(
        &self,
        label: &str,
        mut operation: F,
    ) -> Result<T, OrchestratorError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, OrchestratorError>>,
    {
        // We retry transient remote failures 3 times with exponential backoff and
        // jitter starting at 250ms. That is long enough to smooth over brief
        // network hiccups without stalling a worker for minutes.
        let mut delay = Duration::from_millis(250);
        let mut last_error = None;

        for attempt in 0..3 {
            match operation().await {
                Ok(value) => return Ok(value),
                Err(err) => {
                    last_error = Some(err);
                    if attempt == 2 {
                        break;
                    }
                    let jitter = thread_rng().gen_range(0..150);
                    tracing::info!(operation = label, attempt, "transient failure, retrying");
                    tokio::time::sleep(delay + Duration::from_millis(jitter)).await;
                    delay *= 2;
                }
            }
        }

        Err(last_error
            .unwrap_or_else(|| OrchestratorError::Queue(format!("retry loop failed for {label}"))))
    }
}
