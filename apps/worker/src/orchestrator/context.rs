use hera_db::repos::{
    cases::CaseRepo,
    jobs::ScanJobRepo,
    keys::{StoredViewingKey, ViewKeyRepo},
};
use hera_namada_adapter::{parse_and_validate as parse_namada_view_key, ValidatedNamadaKey};
use hera_types::{Case, ChainId, ScanJob};
use hera_zcash_adapter::{parse_and_validate as parse_zcash_view_key, ValidatedZcashKey};
use uuid::Uuid;

use super::ScanOrchestrator;
use crate::error::OrchestratorError;

/// Bundles the durable records needed to process one scan job so the
/// orchestrator can load them once and pass a single context through helpers.
pub(super) struct LoadedJob {
    pub job: ScanJob,
    pub case: Case,
    pub stored_key: StoredViewingKey,
}

/// Carries the validated chain-specific key so the main orchestrator state
/// machine can stay chain-agnostic after the validation boundary.
pub(super) enum ValidatedChainKey {
    Zcash(ValidatedZcashKey),
    Namada(ValidatedNamadaKey),
}

impl ScanOrchestrator {
    pub(super) async fn load_job_context(
        &self,
        job_id: Uuid,
    ) -> Result<LoadedJob, OrchestratorError> {
        let jobs = ScanJobRepo::new(&self.db);
        let cases = CaseRepo::new(&self.db);
        let keys = ViewKeyRepo::new(&self.db);

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

        Ok(LoadedJob {
            job,
            case,
            stored_key,
        })
    }

    pub(super) async fn validate_chain_key(
        &self,
        loaded: &LoadedJob,
    ) -> Result<ValidatedChainKey, OrchestratorError> {
        let decrypted_key = self.decrypt_view_key(&loaded.stored_key).await?;
        let raw_key = std::str::from_utf8(&decrypted_key)
            .map_err(|_| OrchestratorError::Queue("view key was not valid utf-8".into()))?;

        match loaded.job.chain {
            ChainId::Zcash => Ok(ValidatedChainKey::Zcash(parse_zcash_view_key(
                raw_key,
                loaded.job.network.clone(),
            )?)),
            ChainId::Namada => Ok(ValidatedChainKey::Namada(parse_namada_view_key(
                raw_key,
                &self.config.namada_chain_id,
            )?)),
        }
    }
}
