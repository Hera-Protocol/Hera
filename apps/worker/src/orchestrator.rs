use std::{future::Future, sync::Arc, time::Duration};

use deadpool_redis::Pool as RedisPool;
use ed25519_dalek::SigningKey;
use hera_core::{normalize_namada_note, normalize_zcash_note, NormalizationContext};
use hera_crypto::{decrypt_viewing_key, EncryptedViewingKey, KmsClient};
use hera_db::{
    repos::{
        audit::{AuditAction, AuditEntry, AuditRepo},
        cases::CaseRepo,
        events::EventRepo,
        jobs::ScanJobRepo,
        keys::{StoredViewingKey, ViewKeyRepo},
    },
    DbPool,
};
use hera_namada_adapter::{
    detect_owned_notes, parse_and_validate as parse_namada_view_key, MaspIndexerClient,
    TransferDirection,
};
use hera_reporter::{build_manifest, build_pdf, ReportStorage};
use hera_types::{Case, ChainId, ScanJobStatus};
use hera_zcash_adapter::{parse_and_validate as parse_zcash_view_key, TxMeta, ZcashScanner};
use rand::{thread_rng, Rng};
use tracing::info;
use uuid::Uuid;

use crate::{
    checkpoint::{get_last_checkpoint, save_checkpoint},
    config::Config,
    error::OrchestratorError,
};

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
        let jobs = ScanJobRepo::new(&self.db);
        let cases = CaseRepo::new(&self.db);
        let keys = ViewKeyRepo::new(&self.db);
        let audits = AuditRepo::new(&self.db);
        let events = EventRepo::new(&self.db);

        let job = jobs
            .get_scan_job_by_id(job_id)
            .await?
            .ok_or_else(|| OrchestratorError::MissingRecord(format!("scan job {job_id}")))?;
        let case = cases
            .get_case_by_id(job.case_id)
            .await?
            .ok_or_else(|| OrchestratorError::MissingRecord(format!("case {}", job.case_id)))?;
        let stored_key = keys.get_view_key_for_case(case.id).await?.ok_or_else(|| {
            OrchestratorError::MissingRecord(format!("view key for case {}", case.id))
        })?;

        let outcome: Result<(), OrchestratorError> = async {
            // KEY_VALIDATED decrypts the stored ciphertext and validates the key
            // before any network scan begins.
            self.transition(job.id, ScanJobStatus::KeyValidated).await?;
            self.append_audit(&audits, job.id, AuditAction::KeyRead)
                .await?;
            let decrypted_key = self.decrypt_view_key(&stored_key).await?;
            let raw_key = std::str::from_utf8(&decrypted_key)
                .map_err(|_| OrchestratorError::Queue("view key was not valid utf-8".into()))?;

            match job.chain {
                ChainId::Zcash => {
                    let validated_key = parse_zcash_view_key(raw_key, job.network.clone())?;

                    // CHAIN_SYNCING contacts lightwalletd and persists progress so
                    // interrupted scans resume from the latest checkpoint.
                    self.transition(job.id, ScanJobStatus::ChainSyncing).await?;
                    self.append_audit(&audits, job.id, AuditAction::ScanStarted)
                        .await?;
                    let from_height = get_last_checkpoint(&self.db, case.id, job.chain.clone())
                        .await?
                        .or(stored_key.birthday_height)
                        .or(validated_key.birthday_height.map(u64::from))
                        .unwrap_or(0);
                    let from_height = u32::try_from(from_height).map_err(|_| {
                        OrchestratorError::Queue("zcash checkpoint overflow".into())
                    })?;
                    let scanner = ZcashScanner {
                        lightwalletd_url: self.config.lightwalletd_url.clone(),
                        network: job.network.clone(),
                    };
                    let to_height = self
                        .with_retry("zcash chain tip", || {
                            let scanner = &scanner;
                            async move {
                                scanner
                                    .latest_block_height()
                                    .await
                                    .map_err(OrchestratorError::from)
                            }
                        })
                        .await?;
                    let notes = self
                        .with_retry("zcash scan", || {
                            let scanner = &scanner;
                            let validated_key = validated_key.clone();
                            let db = self.db.clone();
                            let case_id = case.id;
                            let chain = job.chain.clone();
                            async move {
                                scanner
                                    .scan(&validated_key, from_height, to_height, |checkpoint| {
                                        let db = db.clone();
                                        let chain = chain.clone();
                                        tokio::spawn(async move {
                                            let _ = save_checkpoint(
                                                &db,
                                                case_id,
                                                chain,
                                                u64::from(checkpoint.last_scanned_height),
                                            )
                                            .await;
                                        });
                                    })
                                    .await
                                    .map_err(OrchestratorError::from)
                            }
                        })
                        .await?;

                    // DETECTING_NOTES is explicit even for Zcash because trial
                    // decryption and ownership checks are their own failure domain.
                    self.transition(job.id, ScanJobStatus::DetectingNotes)
                        .await?;

                    // CLASSIFYING_FLOWS normalizes adapter notes into canonical
                    // events and stores them idempotently.
                    self.transition(job.id, ScanJobStatus::ClassifyingFlows)
                        .await?;
                    let ctx = NormalizationContext {
                        case_id: case.id,
                        chain: ChainId::Zcash,
                        network: case.network.clone(),
                        scan_engine_version: self.config.scan_engine_version.clone(),
                    };

                    for note in notes {
                        let event = normalize_zcash_note(
                            note.clone(),
                            TxMeta {
                                txid: note.txid.clone(),
                                block_height: note.block_height,
                                timestamp: chrono::Utc::now(),
                                network: case.network.clone(),
                            },
                            &ctx,
                        )?;
                        events.insert_canonical_event(&event).await?;
                    }
                }
                ChainId::Namada => {
                    let validated_key = parse_namada_view_key(raw_key, "namada")?;

                    self.transition(job.id, ScanJobStatus::ChainSyncing).await?;
                    self.append_audit(&audits, job.id, AuditAction::ScanStarted)
                        .await?;
                    let from_block = get_last_checkpoint(&self.db, case.id, job.chain.clone())
                        .await?
                        .or(stored_key.birthday_height)
                        .or(validated_key.birthday_height)
                        .unwrap_or(0);
                    let client = MaspIndexerClient {
                        indexer_url: self.config.namada_indexer_url.clone(),
                    };
                    let context = self
                        .with_retry("namada sync", || {
                            let client = &client;
                            let validated_key = validated_key.clone();
                            let db = self.db.clone();
                            let case_id = case.id;
                            let chain = job.chain.clone();
                            async move {
                                client
                                    .fetch_shielded_context(
                                        &validated_key,
                                        from_block,
                                        |checkpoint| {
                                            let db = db.clone();
                                            let chain = chain.clone();
                                            tokio::spawn(async move {
                                                let _ = save_checkpoint(
                                                    &db,
                                                    case_id,
                                                    chain,
                                                    checkpoint.last_synced_block,
                                                )
                                                .await;
                                            });
                                        },
                                    )
                                    .await
                                    .map_err(OrchestratorError::from)
                            }
                        })
                        .await?;

                    self.transition(job.id, ScanJobStatus::DetectingNotes)
                        .await?;
                    let notes = detect_owned_notes(&context, &validated_key)?;

                    self.transition(job.id, ScanJobStatus::ClassifyingFlows)
                        .await?;
                    let ctx = NormalizationContext {
                        case_id: case.id,
                        chain: ChainId::Namada,
                        network: case.network.clone(),
                        scan_engine_version: self.config.scan_engine_version.clone(),
                    };

                    for note in notes {
                        let event =
                            normalize_namada_note(note, TransferDirection::Shielded, None, &ctx)?;
                        events.insert_canonical_event(&event).await?;
                    }
                }
            }

            // BUILDING_REPORT is explicit because artifact generation can fail
            // independently of scanning and normalization.
            self.transition(job.id, ScanJobStatus::BuildingReport)
                .await?;
            self.build_report(&case).await?;
            self.append_audit(&audits, job.id, AuditAction::ReportExported)
                .await?;

            // SIGNED records successful artifact completion. Prompt 9 will wire
            // the stored artifacts and detached signature behind a final durable
            // job-state transition.
            self.transition(job.id, ScanJobStatus::Signed).await?;
            self.append_audit(&audits, job.id, AuditAction::StatusChanged)
                .await?;
            Ok(())
        }
        .await;

        if let Err(err) = outcome {
            let reason = err.to_string();
            let _ = self.transition(job.id, ScanJobStatus::Failed(reason)).await;
            let _ = self
                .append_audit(&audits, job.id, AuditAction::StatusChanged)
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
        repo: &AuditRepo<'_>,
        job_id: Uuid,
        action: AuditAction,
    ) -> Result<(), OrchestratorError> {
        repo.append_audit_log(AuditEntry {
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

    async fn build_report(&self, case: &Case) -> Result<(), OrchestratorError> {
        let events = EventRepo::new(&self.db).get_events_for_case(case.id).await?;
        let manifest = build_manifest(case, &events, self.report_signing_key.as_ref())?;
        let pdf = build_pdf(&manifest, case)?;
        self.report_storage
            .store_report(case.id, &manifest, &pdf)
            .await?;
        Ok(())
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
