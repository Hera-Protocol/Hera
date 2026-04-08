use hera_core::{normalize_namada_note, normalize_zcash_note, NormalizationContext};
use hera_db::repos::events::EventRepo;
use hera_namada_adapter::{
    detect_owned_notes, MaspIndexerClient, TransferDirection, ValidatedNamadaKey,
};
use hera_types::{Case, ChainId};
use hera_zcash_adapter::{TxMeta, ValidatedZcashKey, ZcashScanner};

use super::{
    context::{LoadedJob, ValidatedChainKey},
    ScanOrchestrator,
};
use crate::{
    checkpoint::{get_last_checkpoint, save_checkpoint},
    error::OrchestratorError,
};

impl ScanOrchestrator {
    pub(super) async fn run_chain_scan(
        &self,
        loaded: &LoadedJob,
        validated_key: ValidatedChainKey,
    ) -> Result<(), OrchestratorError> {
        match validated_key {
            ValidatedChainKey::Zcash(validated_key) => {
                self.process_zcash_scan(loaded, validated_key).await
            }
            ValidatedChainKey::Namada(validated_key) => {
                self.process_namada_scan(loaded, validated_key).await
            }
        }
    }

    pub(super) async fn finalize_report(&self, case: &Case) -> Result<(), OrchestratorError> {
        let events = EventRepo::new(&self.db)
            .get_events_for_case(case.id)
            .await?;
        let manifest =
            hera_reporter::build_manifest(case, &events, self.report_signing_key.as_ref())?;
        let pdf = hera_reporter::build_pdf(&manifest, case)?;
        self.report_storage
            .store_report(case.id, &manifest, &pdf)
            .await?;
        Ok(())
    }

    async fn process_zcash_scan(
        &self,
        loaded: &LoadedJob,
        validated_key: ValidatedZcashKey,
    ) -> Result<(), OrchestratorError> {
        let from_height = get_last_checkpoint(&self.db, loaded.case.id, loaded.job.chain.clone())
            .await?
            .or(loaded.stored_key.birthday_height)
            .or(validated_key.birthday_height.map(u64::from))
            .unwrap_or(0);
        let from_height = u32::try_from(from_height)
            .map_err(|_| OrchestratorError::Queue("zcash checkpoint overflow".into()))?;
        let scanner = ZcashScanner {
            lightwalletd_url: self.config.lightwalletd_url.clone(),
            network: loaded.job.network.clone(),
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
                let case_id = loaded.case.id;
                let chain = loaded.job.chain.clone();
                async move {
                    scanner
                        .scan(&validated_key, from_height, to_height, move |checkpoint| {
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

        self.transition(loaded.job.id, hera_types::ScanJobStatus::DetectingNotes)
            .await?;
        self.transition(loaded.job.id, hera_types::ScanJobStatus::ClassifyingFlows)
            .await?;

        let ctx = NormalizationContext {
            case_id: loaded.case.id,
            chain: ChainId::Zcash,
            network: loaded.case.network.clone(),
            scan_engine_version: self.config.scan_engine_version.clone(),
        };

        for note in notes {
            let event = normalize_zcash_note(
                note.clone(),
                TxMeta {
                    txid: note.txid.clone(),
                    block_height: note.block_height,
                    timestamp: chrono::Utc::now(),
                    network: loaded.case.network.clone(),
                },
                &ctx,
            )?;
            EventRepo::new(&self.db)
                .insert_canonical_event(&event)
                .await?;
        }

        Ok(())
    }

    async fn process_namada_scan(
        &self,
        loaded: &LoadedJob,
        validated_key: ValidatedNamadaKey,
    ) -> Result<(), OrchestratorError> {
        let from_block = get_last_checkpoint(&self.db, loaded.case.id, loaded.job.chain.clone())
            .await?
            .or(loaded.stored_key.birthday_height)
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
                let case_id = loaded.case.id;
                let chain = loaded.job.chain.clone();
                async move {
                    client
                        .fetch_shielded_context(&validated_key, from_block, move |checkpoint| {
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
                        })
                        .await
                        .map_err(OrchestratorError::from)
                }
            })
            .await?;

        self.transition(loaded.job.id, hera_types::ScanJobStatus::DetectingNotes)
            .await?;
        let notes = detect_owned_notes(&context, &validated_key)?;

        self.transition(loaded.job.id, hera_types::ScanJobStatus::ClassifyingFlows)
            .await?;
        let ctx = NormalizationContext {
            case_id: loaded.case.id,
            chain: ChainId::Namada,
            network: loaded.case.network.clone(),
            scan_engine_version: self.config.scan_engine_version.clone(),
        };

        for note in notes {
            let event = normalize_namada_note(note, TransferDirection::Shielded, None, &ctx)?;
            EventRepo::new(&self.db)
                .insert_canonical_event(&event)
                .await?;
        }

        Ok(())
    }
}
