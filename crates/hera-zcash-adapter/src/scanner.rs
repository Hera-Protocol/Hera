use chrono::Utc;
use futures_util::StreamExt;
use hera_types::Network;
use prost::Message;
use zcash_client_backend::proto::{
    compact_formats::{CompactBlock, CompactOrchardAction, CompactSaplingOutput},
    service::{compact_tx_streamer_client::CompactTxStreamerClient, BlockId, BlockRange, Empty},
};

use crate::{
    decryption::trial_decrypt_note,
    error::ZcashAdapterError,
    types::{Pool, ZcashNote, ZcashScanCheckpoint},
    viewing_key::ValidatedZcashKey,
};

/// Holds the minimum connection state needed to scan compact Zcash blocks from
/// a lightwalletd-compatible server.
pub struct ZcashScanner {
    pub lightwalletd_url: String,
    pub network: Network,
}

impl ZcashScanner {
    /// Streams compact blocks from lightwalletd and trial-decrypts each shielded
    /// output. `checkpoint_cb` is called every N blocks so the orchestrator can
    /// persist progress. If this job is interrupted and retried, we resume from
    /// the last checkpoint — not from the start.
    pub async fn scan<F>(
        &self,
        key: &ValidatedZcashKey,
        from_height: u32,
        to_height: u32,
        checkpoint_cb: F,
    ) -> Result<Vec<ZcashNote>, ZcashAdapterError>
    where
        F: Fn(ZcashScanCheckpoint),
    {
        if from_height > to_height {
            return Err(ZcashAdapterError::ScanFailed(
                "from_height must be less than or equal to to_height".into(),
            ));
        }

        let mut client = CompactTxStreamerClient::connect(self.lightwalletd_url.clone())
            .await
            .map_err(|err| ZcashAdapterError::IndexerUnavailable(err.to_string()))?;

        // We call `GetLightdInfo` first because it tells us which chain the server
        // thinks it serves. That prevents a valid key from being scanned against
        // the wrong network endpoint.
        let lightd_info = client
            .get_lightd_info(Empty {})
            .await
            .map_err(|err| ZcashAdapterError::IndexerUnavailable(err.to_string()))?
            .into_inner();

        self.validate_chain_name(&lightd_info.chain_name)?;

        // We call `GetBlockRange` because lightwalletd is designed to stream
        // compact blocks efficiently; downloading full blocks would waste bandwidth
        // and defeat the point of the light client protocol.
        let block_range = BlockRange {
            start: Some(BlockId {
                height: u64::from(from_height),
                hash: vec![],
            }),
            end: Some(BlockId {
                height: u64::from(to_height),
                hash: vec![],
            }),
        };

        let mut stream = client
            .get_block_range(block_range)
            .await
            .map_err(|err| ZcashAdapterError::IndexerUnavailable(err.to_string()))?
            .into_inner();

        let mut notes = Vec::new();
        let mut blocks_since_checkpoint = 0u32;

        while let Some(block) = stream.next().await {
            let block = block.map_err(|err| ZcashAdapterError::ScanFailed(err.to_string()))?;
            notes.extend(self.scan_block(&block, key)?);

            blocks_since_checkpoint = blocks_since_checkpoint.saturating_add(1);
            if blocks_since_checkpoint >= 100 {
                checkpoint_cb(ZcashScanCheckpoint {
                    last_scanned_height: u32::try_from(block.height).map_err(|_| {
                        ZcashAdapterError::ScanFailed("block height overflow".into())
                    })?,
                    scanned_at: Utc::now(),
                });
                blocks_since_checkpoint = 0;
            }
        }

        if to_height >= from_height {
            checkpoint_cb(ZcashScanCheckpoint {
                last_scanned_height: to_height,
                scanned_at: Utc::now(),
            });
        }

        Ok(notes)
    }

    fn scan_block(
        &self,
        block: &CompactBlock,
        key: &ValidatedZcashKey,
    ) -> Result<Vec<ZcashNote>, ZcashAdapterError> {
        let mut notes = Vec::new();

        for tx in &block.vtx {
            // Sapling and Orchard are scanned as separate flows because a failure in
            // one pool must not be mistaken for successful complete coverage.
            for output in &tx.outputs {
                if let Some(mut note) = self.scan_sapling_output(output, key)? {
                    note.txid = hex::encode(&tx.hash);
                    note.block_height = u32::try_from(block.height).map_err(|_| {
                        ZcashAdapterError::ScanFailed("block height overflow".into())
                    })?;
                    notes.push(note);
                }
            }

            for action in &tx.actions {
                if let Some(mut note) = self.scan_orchard_action(action, key)? {
                    note.txid = hex::encode(&tx.hash);
                    note.block_height = u32::try_from(block.height).map_err(|_| {
                        ZcashAdapterError::ScanFailed("block height overflow".into())
                    })?;
                    notes.push(note);
                }
            }
        }

        Ok(notes)
    }

    fn scan_sapling_output(
        &self,
        output: &CompactSaplingOutput,
        key: &ValidatedZcashKey,
    ) -> Result<Option<ZcashNote>, ZcashAdapterError> {
        let bytes = output.encode_to_vec();
        Ok(trial_decrypt_note(&bytes, key, Pool::Sapling))
    }

    fn scan_orchard_action(
        &self,
        action: &CompactOrchardAction,
        key: &ValidatedZcashKey,
    ) -> Result<Option<ZcashNote>, ZcashAdapterError> {
        let bytes = action.encode_to_vec();
        Ok(trial_decrypt_note(&bytes, key, Pool::Orchard))
    }

    fn validate_chain_name(&self, chain_name: &str) -> Result<(), ZcashAdapterError> {
        let lower = chain_name.to_ascii_lowercase();
        let matches_network = match self.network {
            Network::Mainnet => lower.contains("main"),
            Network::Testnet => lower.contains("test"),
            Network::Regtest => lower.contains("regtest") || lower.contains("test"),
        };

        if matches_network {
            Ok(())
        } else {
            Err(ZcashAdapterError::ScanFailed(format!(
                "lightwalletd chain '{}' does not match requested network",
                chain_name
            )))
        }
    }
}
