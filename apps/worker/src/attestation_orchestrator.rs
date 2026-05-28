use std::sync::Arc;

use deadpool_redis::Pool as RedisPool;
use hera_db::{
    repos::{
        attestation_jobs::AttestationJobRepo,
        events::EventRepo,
    },
    DbPool,
};
use hera_proof_circuits::{
    caulk::srs::CaulkPlusSrs,
    circuits::{
        blocklist::{
            prove_no_blocklist_exposure, verify_no_blocklist_exposure, BlocklistStatement,
        },
        risk_score::{prove_risk_below, verify_risk_below, RiskScoreStatement},
        threshold::{prove_threshold, verify_threshold, ThresholdStatement},
    },
};
use hera_proof_witness::WitnessBuilder;
use hera_reporter::AttestationStorage;
use hera_types::AttestationJobStatus;
use tracing::info;
use uuid::Uuid;

use crate::{config::Config, error::OrchestratorError};

pub struct AttestationOrchestrator {
    pub db: DbPool,
    pub _redis: RedisPool,
    pub attestation_storage: Arc<AttestationStorage>,
    pub srs: Arc<CaulkPlusSrs>,
    pub config: Config,
}

impl AttestationOrchestrator {
    pub async fn process_job(&self, job_id: Uuid) -> Result<(), OrchestratorError> {
        let job = AttestationJobRepo::new(&self.db)
            .get_by_id(job_id)
            .await?
            .ok_or_else(|| {
                OrchestratorError::MissingRecord(format!("attestation job {job_id}"))
            })?;

        let outcome: Result<(), OrchestratorError> = async {
            // WitnessBuilding: load events and build witness records.
            self.transition(job_id, AttestationJobStatus::WitnessBuilding)
                .await?;

            let events = EventRepo::new(&self.db)
                .get_events_for_case(job.case_id)
                .await?;

            if events.is_empty() {
                return Err(OrchestratorError::MissingRecord(
                    "no canonical events for case".to_string(),
                ));
            }

            // Extract risk scores from parameters if provided.
            let risk_scores = extract_risk_scores(&job.parameters);
            let witness_records = WitnessBuilder::build(&events, &risk_scores)?;

            // Proving: generate the proof based on proof_type.
            self.transition(job_id, AttestationJobStatus::Proving)
                .await?;

            let mut rng = ark_std::test_rng();
            let (proof_bytes, public_inputs) = match job.proof_type.as_str() {
                "THRESHOLD_RECEIVED" => {
                    let threshold_raw = job.parameters["threshold"]
                        .as_u64()
                        .unwrap_or(0) as u128;
                    let statement = ThresholdStatement {
                        threshold_raw,
                        event_count: witness_records.len(),
                    };
                    let proof =
                        prove_threshold(&self.srs, &witness_records, &statement, &mut rng)?;
                    let valid = verify_threshold(&self.srs, &statement, &proof)?;
                    let inputs = serde_json::json!({
                        "proof_type": "THRESHOLD_RECEIVED",
                        "threshold": threshold_raw,
                        "event_count": witness_records.len(),
                        "verified": valid,
                    });
                    (proof.caulk_proof.to_bytes()?, inputs)
                }
                "NO_BLOCKLIST_EXPOSURE" => {
                    let blocklist: Vec<ark_bls12_381::Fr> = job.parameters["blocklist"]
                        .as_array()
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_u64())
                                .map(ark_bls12_381::Fr::from)
                                .collect()
                        })
                        .unwrap_or_default();
                    let statement = BlocklistStatement {
                        blocklist_size: blocklist.len(),
                        event_count: witness_records.len(),
                    };
                    let proof = prove_no_blocklist_exposure(
                        &self.srs,
                        &witness_records,
                        &blocklist,
                        &mut rng,
                    )?;
                    let valid = verify_no_blocklist_exposure(&self.srs, &statement, &proof)?;
                    let inputs = serde_json::json!({
                        "proof_type": "NO_BLOCKLIST_EXPOSURE",
                        "blocklist_size": blocklist.len(),
                        "event_count": witness_records.len(),
                        "verified": valid,
                    });
                    (proof.membership_proof.to_bytes()?, inputs)
                }
                "RISK_BELOW_THRESHOLD" => {
                    let max_risk = job.parameters["max_risk"].as_u64().unwrap_or(100) as u8;
                    let statement = RiskScoreStatement {
                        max_risk,
                        event_count: witness_records.len(),
                    };
                    let proof =
                        prove_risk_below(&self.srs, &witness_records, &statement, &mut rng)?;
                    let valid = verify_risk_below(&self.srs, &statement, &proof)?;
                    let inputs = serde_json::json!({
                        "proof_type": "RISK_BELOW_THRESHOLD",
                        "max_risk": max_risk,
                        "event_count": witness_records.len(),
                        "verified": valid,
                    });
                    (proof.caulk_proof.to_bytes()?, inputs)
                }
                other => {
                    return Err(OrchestratorError::MissingRecord(format!(
                        "unknown proof type: {other}"
                    )));
                }
            };

            // Upload proof to S3 and store attestation record.
            self.attestation_storage
                .store_attestation(
                    job.case_id,
                    job_id,
                    &job.proof_type,
                    &proof_bytes,
                    &public_inputs,
                    &self.config.proof_engine_version,
                )
                .await
                .map_err(OrchestratorError::Reporter)?;

            self.transition(job_id, AttestationJobStatus::Verified)
                .await?;
            info!(%job_id, "attestation job completed successfully");
            Ok(())
        }
        .await;

        if let Err(err) = outcome {
            let reason = err.to_string();
            let _ = self
                .transition(job_id, AttestationJobStatus::Failed(reason))
                .await;
            return Err(err);
        }

        Ok(())
    }

    async fn transition(
        &self,
        job_id: Uuid,
        status: AttestationJobStatus,
    ) -> Result<(), OrchestratorError> {
        info!(%job_id, status = ?status, "attestation job transition");
        AttestationJobRepo::new(&self.db)
            .update_status(job_id, status)
            .await?
            .ok_or_else(|| {
                OrchestratorError::MissingRecord(format!("attestation job {job_id}"))
            })?;
        Ok(())
    }
}

fn extract_risk_scores(
    parameters: &serde_json::Value,
) -> std::collections::HashMap<uuid::Uuid, u8> {
    let mut scores = std::collections::HashMap::new();
    if let Some(obj) = parameters.get("risk_scores").and_then(|v| v.as_object()) {
        for (key, val) in obj {
            if let (Ok(uuid), Some(score)) = (key.parse::<uuid::Uuid>(), val.as_u64()) {
                scores.insert(uuid, score as u8);
            }
        }
    }
    scores
}
