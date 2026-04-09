use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;

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
    public_state: Option<PublicIndexerState>,
    last_synced_epoch: u64,
    last_synced_block: u64,
}

/// Captures the public MASP indexer's raw transaction window and auxiliary
/// state snapshots. We retain the raw JSON payloads because the official Namada
/// client consumes multiple endpoints together when reconstructing owned-note
/// state; preserving them here avoids baking unstable wire details into the
/// rest of Hera prematurely.
#[derive(Debug, Clone)]
pub struct PublicIndexerState {
    txs: Vec<IndexedMaspTx>,
    note_index: Value,
    witness_map: Value,
    commitment_tree: Value,
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
            public_state: None,
            last_synced_epoch,
            last_synced_block,
        }
    }

    pub(crate) fn new_public(
        public_state: PublicIndexerState,
        last_synced_epoch: u64,
        last_synced_block: u64,
    ) -> Self {
        Self {
            entries: Vec::new(),
            public_state: Some(public_state),
            last_synced_epoch,
            last_synced_block,
        }
    }

    pub(crate) fn entries(&self) -> &[ShieldedEntry] {
        &self.entries
    }

    pub fn public_state(&self) -> Option<&PublicIndexerState> {
        self.public_state.as_ref()
    }

    pub fn last_synced_epoch(&self) -> u64 {
        self.last_synced_epoch
    }

    pub fn last_synced_block(&self) -> u64 {
        self.last_synced_block
    }
}

impl PublicIndexerState {
    pub(crate) fn new(
        txs: Vec<IndexedMaspTx>,
        note_index: Value,
        witness_map: Value,
        commitment_tree: Value,
    ) -> Self {
        Self {
            txs,
            note_index,
            witness_map,
            commitment_tree,
        }
    }

    pub fn tx_count(&self) -> usize {
        self.txs.len()
    }

    pub fn batch_count(&self) -> usize {
        self.txs.iter().map(|tx| tx.batch.len()).sum::<usize>()
    }

    pub fn fee_batch_count(&self) -> usize {
        self.txs
            .iter()
            .flat_map(|tx| tx.batch.iter())
            .filter(|batch| batch.is_masp_fee_payment)
            .count()
    }

    pub fn total_byte_len(&self) -> usize {
        self.txs
            .iter()
            .flat_map(|tx| tx.batch.iter())
            .map(|batch| batch.bytes.len())
            .sum::<usize>()
    }

    pub fn block_index_sum(&self) -> u64 {
        self.txs.iter().map(|tx| tx.block_index).sum::<u64>()
    }

    pub fn highest_masp_tx_index(&self) -> u64 {
        self.txs
            .iter()
            .flat_map(|tx| tx.batch.iter())
            .map(|batch| batch.masp_tx_index)
            .max()
            .unwrap_or(0)
    }

    pub fn block_heights(&self) -> Vec<u64> {
        self.txs
            .iter()
            .map(|tx| tx.block_height)
            .collect::<Vec<_>>()
    }

    pub fn note_index_entries(&self) -> usize {
        payload_size_hint(&self.note_index)
    }

    pub fn witness_entries(&self) -> usize {
        payload_size_hint(&self.witness_map)
    }

    pub fn commitment_tree_entries(&self) -> usize {
        payload_size_hint(&self.commitment_tree)
    }

    pub fn has_auxiliary_state(&self) -> bool {
        !self.note_index.is_null() && !self.witness_map.is_null() && !self.commitment_tree.is_null()
    }

    pub fn summary(&self) -> String {
        format!(
            "{} indexed transactions, {} MASP batches, {} fee batches, {} raw bytes, block-index sum {}, highest MASP tx index {}, note-index size {}, witness-map size {}, commitment-tree size {}",
            self.tx_count(),
            self.batch_count(),
            self.fee_batch_count(),
            self.total_byte_len(),
            self.block_index_sum(),
            self.highest_masp_tx_index(),
            self.note_index_entries(),
            self.witness_entries(),
            self.commitment_tree_entries(),
        )
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
    /// load the transaction range plus the auxiliary note-index, witness-map,
    /// and commitment-tree snapshots that the official Namada client also
    /// relies on. Hera still performs owned-note detection locally so the
    /// viewing key never leaves the isolated execution boundary.
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

        let note_index = self
            .fetch_json_value(
                &client,
                "notes-index",
                vec![("height", latest.height.to_string())],
            )
            .await?;
        let witness_map = self
            .fetch_json_value(
                &client,
                "witness-map",
                vec![("height", latest.height.to_string())],
            )
            .await?;
        let commitment_tree = self
            .fetch_json_value(
                &client,
                "commitment-tree",
                vec![("height", latest.height.to_string())],
            )
            .await?;

        let checkpoint = NamadaScanCheckpoint {
            last_synced_epoch: latest.epoch,
            last_synced_block: latest.height,
        };
        checkpoint_cb(checkpoint.clone());

        Ok(ShieldedContext::new_public(
            PublicIndexerState::new(response.txs, note_index, witness_map, commitment_tree),
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

    async fn fetch_json_value(
        &self,
        client: &Client,
        path: &str,
        query: Vec<(&str, String)>,
    ) -> Result<Value, NamadaAdapterError> {
        client
            .get(format!("{}/{}", self.api_base(), path))
            .query(&query)
            .send()
            .await
            .map_err(|err| NamadaAdapterError::IndexerUnavailable(err.to_string()))?
            .error_for_status()
            .map_err(|err| NamadaAdapterError::MaspSyncFailed(err.to_string()))?
            .json::<Value>()
            .await
            .map_err(|err| NamadaAdapterError::MaspSyncFailed(err.to_string()))
    }
}

fn payload_size_hint(value: &Value) -> usize {
    match value {
        Value::Array(items) => items.len(),
        Value::Object(entries) => entries.len(),
        Value::Null => 0,
        _ => 1,
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
    use serde_json::json;

    use super::{MaspIndexerClient, PublicIndexerState};

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

    #[test]
    fn public_indexer_state_reports_payload_sizes() {
        let state = PublicIndexerState::new(
            vec![super::IndexedMaspTx {
                block_height: 12,
                block_index: 0,
                batch: vec![super::IndexedMaspBatchItem {
                    masp_tx_index: 0,
                    is_masp_fee_payment: false,
                    bytes: vec![1, 2, 3, 4],
                }],
            }],
            json!([{ "note": 1 }, { "note": 2 }]),
            json!({ "w1": { "root": "abc" } }),
            json!([{ "node": 1 }, { "node": 2 }, { "node": 3 }]),
        );

        assert!(state.has_auxiliary_state());
        assert_eq!(state.tx_count(), 1);
        assert_eq!(state.batch_count(), 1);
        assert_eq!(state.total_byte_len(), 4);
        assert_eq!(state.block_index_sum(), 0);
        assert_eq!(state.highest_masp_tx_index(), 0);
        assert_eq!(state.block_heights(), vec![12]);
        assert_eq!(state.note_index_entries(), 2);
        assert_eq!(state.witness_entries(), 1);
        assert_eq!(state.commitment_tree_entries(), 3);
    }
}
