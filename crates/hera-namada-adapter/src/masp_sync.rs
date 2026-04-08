use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{
    error::NamadaAdapterError, types::NamadaScanCheckpoint, viewing_key::ValidatedNamadaKey,
};

/// Wraps the raw MASP sync response so other modules can inspect note candidates
/// without coupling to indexer-specific JSON wire details. We use indexer-first
/// sync because full-node MASP scanning is expensive. The indexer pre-filters
/// relevant transactions.
#[derive(Debug, Clone)]
pub struct ShieldedContext {
    entries: Vec<ShieldedEntry>,
    last_synced_epoch: u64,
    last_synced_block: u64,
}

impl ShieldedContext {
    pub(crate) fn new(
        entries: Vec<ShieldedEntry>,
        last_synced_epoch: u64,
        last_synced_block: u64,
    ) -> Self {
        Self {
            entries,
            last_synced_epoch,
            last_synced_block,
        }
    }

    pub(crate) fn entries(&self) -> &[ShieldedEntry] {
        &self.entries
    }

    pub fn last_synced_epoch(&self) -> u64 {
        self.last_synced_epoch
    }

    pub fn last_synced_block(&self) -> u64 {
        self.last_synced_block
    }
}

/// Talks to the MASP indexer instead of a full node because indexer-first sync
/// is the scalable path for Stage 1.
pub struct MaspIndexerClient {
    pub indexer_url: String,
}

impl MaspIndexerClient {
    /// Fetches the pre-filtered shielded context for a viewing key. We call
    /// `GET /api/v1/block/latest` first to learn the indexer's current cursor,
    /// then `POST /api/v1/masp/sync` to request MASP-relevant entries for the
    /// viewing key and block window. The second endpoint shape is an intentional
    /// Stage 1 assumption that should be aligned with the deployed indexer swagger.
    pub async fn fetch_shielded_context<F>(
        &self,
        key: &ValidatedNamadaKey,
        from_block: u64,
        checkpoint_cb: F,
    ) -> Result<ShieldedContext, NamadaAdapterError>
    where
        F: Fn(NamadaScanCheckpoint),
    {
        let client = Client::new();
        let base = self.indexer_url.trim_end_matches('/');

        let latest = client
            .get(format!("{base}/api/v1/block/latest"))
            .send()
            .await
            .map_err(|err| NamadaAdapterError::IndexerUnavailable(err.to_string()))?
            .error_for_status()
            .map_err(|err| NamadaAdapterError::IndexerUnavailable(err.to_string()))?
            .json::<LatestBlockResponse>()
            .await
            .map_err(|err| NamadaAdapterError::MaspSyncFailed(err.to_string()))?;

        let response = client
            .post(format!("{base}/api/v1/masp/sync"))
            .json(&ShieldedSyncRequest {
                viewing_key: key.raw_key.clone(),
                chain_id: key.chain_id.clone(),
                from_block,
            })
            .send()
            .await
            .map_err(|err| NamadaAdapterError::IndexerUnavailable(err.to_string()))?
            .error_for_status()
            .map_err(|err| NamadaAdapterError::MaspSyncFailed(err.to_string()))?
            .json::<ShieldedSyncResponse>()
            .await
            .map_err(|err| NamadaAdapterError::MaspSyncFailed(err.to_string()))?;

        let checkpoint = NamadaScanCheckpoint {
            last_synced_epoch: response.last_epoch.unwrap_or(latest.epoch),
            last_synced_block: response.last_block.unwrap_or(latest.height),
        };
        checkpoint_cb(checkpoint.clone());

        Ok(ShieldedContext::new(
            response.entries,
            checkpoint.last_synced_epoch,
            checkpoint.last_synced_block,
        ))
    }
}

#[derive(Debug, Clone, Deserialize)]
struct LatestBlockResponse {
    epoch: u64,
    height: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct ShieldedSyncRequest {
    viewing_key: String,
    chain_id: String,
    from_block: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ShieldedSyncResponse {
    last_epoch: Option<u64>,
    last_block: Option<u64>,
    entries: Vec<ShieldedEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct ShieldedEntry {
    pub txid: String,
    pub block_height: u64,
    pub asset_id: String,
    pub asset_symbol: Option<String>,
    pub asset_decimals: Option<u8>,
    pub amount_raw: String,
    pub note_commitment: String,
}
