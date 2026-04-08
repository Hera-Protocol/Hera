use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;

use crate::{
    error::NamadaAdapterError, types::NamadaScanCheckpoint, viewing_key::ValidatedNamadaKey,
};

/// Wraps the raw MASP sync response so other modules can inspect note candidates
/// without coupling to indexer-specific JSON wire details. We use indexer-first
/// sync because full-node MASP scanning is expensive. The public indexer gives
/// us raw MASP transaction batches, and private deployments may provide richer
/// pre-decoded note entries.
#[derive(Debug, Clone)]
pub struct ShieldedContext {
    entries: Vec<ShieldedEntry>,
    txs: Vec<IndexedMaspTx>,
    last_synced_epoch: u64,
    last_synced_block: u64,
}

impl ShieldedContext {
    #[cfg(test)]
    pub(crate) fn new_legacy(
        entries: Vec<ShieldedEntry>,
        last_synced_epoch: u64,
        last_synced_block: u64,
    ) -> Self {
        Self {
            entries,
            txs: Vec::new(),
            last_synced_epoch,
            last_synced_block,
        }
    }

    pub(crate) fn new_public(
        txs: Vec<IndexedMaspTx>,
        last_synced_epoch: u64,
        last_synced_block: u64,
    ) -> Self {
        Self {
            entries: Vec::new(),
            txs,
            last_synced_epoch,
            last_synced_block,
        }
    }

    pub(crate) fn entries(&self) -> &[ShieldedEntry] {
        &self.entries
    }

    pub(crate) fn txs(&self) -> &[IndexedMaspTx] {
        &self.txs
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

/// Captures the indexer's latest processed cursor so callers can distinguish
/// endpoint reachability from full MASP sync behavior.
#[derive(Debug, Clone)]
pub struct IndexerCursor {
    pub epoch: u64,
    pub height: u64,
}

impl MaspIndexerClient {
    /// Reads the indexer's latest block cursor from `GET /api/v1/height`
    /// so smoke tests and orchestration can confirm the service is reachable
    /// before asking it for heavier MASP context.
    pub async fn latest_indexed_block(&self) -> Result<IndexerCursor, NamadaAdapterError> {
        let client = Client::new();
        let base = self.api_base();
        let latest = client
            .get(format!("{base}/height"))
            .send()
            .await
            .map_err(|err| NamadaAdapterError::IndexerUnavailable(err.to_string()))?
            .error_for_status()
            .map_err(|err| NamadaAdapterError::IndexerUnavailable(err.to_string()))?
            .json::<LatestHeightResponse>()
            .await
            .map_err(|err| NamadaAdapterError::MaspSyncFailed(err.to_string()))?;

        Ok(IndexerCursor {
            // The public MASP indexer exposes the latest indexed block height
            // but not an epoch cursor, so we conservatively mirror block height
            // into the epoch field until a richer public endpoint is available.
            epoch: latest.block_height,
            height: latest.block_height,
        })
    }

    /// Fetches the shielded context for a viewing key. Public MASP indexers
    /// expose raw transaction windows via `GET /api/v1/tx`, not a pre-filtered
    /// per-viewing-key sync endpoint. We fetch the latest height first, then
    /// load the MASP transaction range client-side so later stages can perform
    /// ownership detection locally.
    pub async fn fetch_shielded_context<F>(
        &self,
        _key: &ValidatedNamadaKey,
        from_block: u64,
        checkpoint_cb: F,
    ) -> Result<ShieldedContext, NamadaAdapterError>
    where
        F: Fn(NamadaScanCheckpoint),
    {
        let client = Client::new();
        let base = self.api_base();
        let latest = self.latest_indexed_block().await?;
        let height_offset = latest.height.saturating_sub(from_block);

        let response = client
            .get(format!("{base}/tx"))
            .query(&[
                ("height", from_block.to_string()),
                ("height_offset", height_offset.to_string()),
            ])
            .send()
            .await
            .map_err(|err| NamadaAdapterError::IndexerUnavailable(err.to_string()))?
            .error_for_status()
            .map_err(|err| NamadaAdapterError::MaspSyncFailed(err.to_string()))?
            .json::<TxResponse>()
            .await
            .map_err(|err| NamadaAdapterError::MaspSyncFailed(err.to_string()))?;

        let checkpoint = NamadaScanCheckpoint {
            last_synced_epoch: latest.epoch,
            last_synced_block: latest.height,
        };
        checkpoint_cb(checkpoint.clone());

        Ok(ShieldedContext::new_public(
            response.txs,
            checkpoint.last_synced_epoch,
            checkpoint.last_synced_block,
        ))
    }

    fn api_base(&self) -> String {
        let base = self.indexer_url.trim_end_matches('/');
        if base.ends_with("/api/v1") {
            base.to_string()
        } else {
            format!("{base}/api/v1")
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct LatestHeightResponse {
    block_height: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct TxResponse {
    txs: Vec<IndexedMaspTx>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct IndexedMaspTx {
    pub block_height: u64,
    pub block_index: u64,
    pub batch: Vec<IndexedMaspBatchItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct IndexedMaspBatchItem {
    pub masp_tx_index: u64,
    pub is_masp_fee_payment: bool,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) struct ShieldedEntry {
    pub txid: String,
    pub block_height: u64,
    pub timestamp: DateTime<Utc>,
    pub asset_id: String,
    pub asset_symbol: Option<String>,
    pub asset_decimals: Option<u8>,
    pub amount_raw: String,
    pub note_commitment: String,
}

#[cfg(test)]
mod tests {
    use super::MaspIndexerClient;

    #[test]
    fn appends_api_prefix_for_public_host() {
        let client = MaspIndexerClient {
            indexer_url: "https://masp.namada.net".into(),
        };

        assert_eq!(client.api_base(), "https://masp.namada.net/api/v1");
    }

    #[test]
    fn preserves_api_prefix_when_already_present() {
        let client = MaspIndexerClient {
            indexer_url: "https://masp.namada.net/api/v1".into(),
        };

        assert_eq!(client.api_base(), "https://masp.namada.net/api/v1");
    }
}
