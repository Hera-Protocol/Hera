use std::collections::HashMap;

use chrono::Utc;
use futures_util::StreamExt;
use hera_types::Network;
use orchard::{
    keys::IncomingViewingKey as OrchardIncomingViewingKey,
    note::Nullifier as OrchardNullifier,
    note_encryption::OrchardDomain,
};
use sapling::{
    note_encryption::SaplingDomain, zip32::IncomingViewingKey as SaplingIncomingViewingKey,
    Nullifier as SaplingNullifier,
};
use zcash_client_backend::proto::{
    compact_formats::CompactBlock,
    service::{compact_tx_streamer_client::CompactTxStreamerClient, BlockId, BlockRange, ChainSpec, Empty},
};
use zcash_client_backend::{
    data_api::BlockMetadata,
    scanning::{scan_block, Nullifiers, ScanningKeyOps, ScanningKeys},
};
use zcash_keys::keys::{UnifiedFullViewingKey, UnifiedIncomingViewingKey};
use zcash_protocol::consensus::{MAIN_NETWORK, TEST_NETWORK};

use crate::{
    error::ZcashAdapterError,
    types::{Pool, ZcashNote, ZcashScanCheckpoint},
    viewing_key::{KeyScope, ValidatedZcashKey},
};

/// Holds the minimum connection state needed to scan compact Zcash blocks from
/// a lightwalletd-compatible server.
pub struct ZcashScanner {
    pub lightwalletd_url: String,
    pub network: Network,
}

impl ZcashScanner {
    /// Queries the remote tip height before a scan so the orchestrator can bind
    /// a scan window to a concrete chain state instead of guessing an end block.
    pub async fn latest_block_height(&self) -> Result<u32, ZcashAdapterError> {
        let mut client = CompactTxStreamerClient::connect(self.lightwalletd_url.clone())
            .await
            .map_err(|err| ZcashAdapterError::IndexerUnavailable(err.to_string()))?;

        let block = client
            .get_latest_block(ChainSpec {})
            .await
            .map_err(|err| ZcashAdapterError::IndexerUnavailable(err.to_string()))?
            .into_inner();

        u32::try_from(block.height)
            .map_err(|_| ZcashAdapterError::ScanFailed("tip height overflow".into()))
    }

    /// Streams compact blocks from lightwalletd and scans them with the official
    /// compact-block trial-decryption engine. We call `GetBlockRange` because
    /// that is the lightwalletd primitive built for shielded scanning; pulling
    /// full blocks would cost more bandwidth without improving note detection.
    ///
    /// `checkpoint_cb` is called every N blocks so the orchestrator can persist
    /// progress. If this job is interrupted and retried, we resume from the
    /// last checkpoint — not from the start.
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
        let mut prior_block_metadata = None;

        while let Some(block) = stream.next().await {
            let block = block.map_err(|err| ZcashAdapterError::ScanFailed(err.to_string()))?;
            let scanned = self.scan_compact_block(block, key, prior_block_metadata.as_ref())?;
            prior_block_metadata = Some(scanned.to_block_metadata());
            notes.extend(self.extract_notes(&scanned)?);

            blocks_since_checkpoint = blocks_since_checkpoint.saturating_add(1);
            if blocks_since_checkpoint >= 100 {
                checkpoint_cb(ZcashScanCheckpoint {
                    last_scanned_height: u32::from(prior_block_metadata
                        .as_ref()
                        .ok_or_else(|| ZcashAdapterError::ScanFailed("missing scanned block metadata".into()))?
                        .block_height()),
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

    fn scan_compact_block(
        &self,
        block: CompactBlock,
        key: &ValidatedZcashKey,
        prior_block_metadata: Option<&BlockMetadata>,
    ) -> Result<zcash_client_backend::data_api::ScannedBlock<u32>, ZcashAdapterError> {
        match key.key_scope {
            KeyScope::Full => self.scan_with_full_view_key(block, key, prior_block_metadata),
            KeyScope::Incoming => {
                self.scan_with_incoming_view_key(block, key, prior_block_metadata)
            }
        }
    }

    fn scan_with_full_view_key(
        &self,
        block: CompactBlock,
        key: &ValidatedZcashKey,
        prior_block_metadata: Option<&BlockMetadata>,
    ) -> Result<zcash_client_backend::data_api::ScannedBlock<u32>, ZcashAdapterError> {
        let ufvk = self.decode_ufvk(&key.raw_key)?;
        let scanning_keys = ScanningKeys::from_account_ufvks([(0u32, ufvk)]);
        match self.network {
            Network::Mainnet => scan_block(
                &MAIN_NETWORK,
                block,
                &scanning_keys,
                &Nullifiers::empty(),
                prior_block_metadata,
            ),
            Network::Testnet | Network::Regtest => scan_block(
                &TEST_NETWORK,
                block,
                &scanning_keys,
                &Nullifiers::empty(),
                prior_block_metadata,
            ),
        }
        .map_err(|err| ZcashAdapterError::ScanFailed(err.to_string()))
    }

    fn scan_with_incoming_view_key(
        &self,
        block: CompactBlock,
        key: &ValidatedZcashKey,
        prior_block_metadata: Option<&BlockMetadata>,
    ) -> Result<zcash_client_backend::data_api::ScannedBlock<u32>, ZcashAdapterError> {
        let uivk = self.decode_uivk(&key.raw_key)?;
        let mut sapling_keys: HashMap<u8, Box<dyn ScanningKeyOps<SaplingDomain, u32, SaplingNullifier>>> =
            HashMap::new();
        let mut orchard_keys: HashMap<u8, Box<dyn ScanningKeyOps<OrchardDomain, u32, OrchardNullifier>>> =
            HashMap::new();

        if let Some(sapling_ivk) = uivk.sapling().clone() {
            sapling_keys.insert(
                1,
                Box::new(IncomingSaplingScannerKey {
                    account_id: 0,
                    ivk: sapling_ivk,
                }),
            );
        }
        if let Some(orchard_ivk) = uivk.orchard().clone() {
            orchard_keys.insert(
                2,
                Box::new(IncomingOrchardScannerKey {
                    account_id: 0,
                    ivk: orchard_ivk,
                }),
            );
        }

        if sapling_keys.is_empty() && orchard_keys.is_empty() {
            return Err(ZcashAdapterError::InvalidViewingKey(
                "incoming viewing key does not contain a Sapling or Orchard receiver".into(),
            ));
        }

        let scanning_keys = ScanningKeys::new(sapling_keys, orchard_keys);
        match self.network {
            Network::Mainnet => scan_block(
                &MAIN_NETWORK,
                block,
                &scanning_keys,
                &Nullifiers::empty(),
                prior_block_metadata,
            ),
            Network::Testnet | Network::Regtest => scan_block(
                &TEST_NETWORK,
                block,
                &scanning_keys,
                &Nullifiers::empty(),
                prior_block_metadata,
            ),
        }
        .map_err(|err| ZcashAdapterError::ScanFailed(err.to_string()))
    }

    fn extract_notes(
        &self,
        scanned_block: &zcash_client_backend::data_api::ScannedBlock<u32>,
    ) -> Result<Vec<ZcashNote>, ZcashAdapterError> {
        let height = u32::from(scanned_block.height());
        let mut notes = Vec::new();

        for tx in scanned_block.transactions() {
            let txid = tx.txid().to_string();

            for output in tx.sapling_outputs() {
                notes.push(ZcashNote {
                    txid: txid.clone(),
                    block_height: height,
                    pool: Pool::Sapling,
                    // Values stay in zatoshis in the adapter to avoid any float
                    // conversion before normalization renders the canonical decimal string.
                    amount_zatoshis: output.note().value().inner(),
                    // Compact block scanning does not reveal decrypted memo contents, so
                    // we mark memo visibility as encrypted instead of pretending it was absent.
                    memo: crate::types::MemoVisibility::Encrypted,
                    nullifier: output.nf().map(|nf| hex::encode(nf.to_vec())),
                });
            }

            for output in tx.orchard_outputs() {
                notes.push(ZcashNote {
                    txid: txid.clone(),
                    block_height: height,
                    pool: Pool::Orchard,
                    amount_zatoshis: output.note().value().inner(),
                    memo: crate::types::MemoVisibility::Encrypted,
                    // Orchard nullifiers are optional here. We omit them rather than
                    // inventing a serialization path that might diverge from consensus.
                    nullifier: None,
                });
            }
        }
        Ok(notes)
    }

    fn decode_ufvk(&self, raw_key: &str) -> Result<UnifiedFullViewingKey, ZcashAdapterError> {
        match self.network {
            Network::Mainnet => UnifiedFullViewingKey::decode(&MAIN_NETWORK, raw_key),
            Network::Testnet | Network::Regtest => {
                UnifiedFullViewingKey::decode(&TEST_NETWORK, raw_key)
            }
        }
        .map_err(ZcashAdapterError::InvalidViewingKey)
    }

    fn decode_uivk(&self, raw_key: &str) -> Result<UnifiedIncomingViewingKey, ZcashAdapterError> {
        match self.network {
            Network::Mainnet => UnifiedIncomingViewingKey::decode(&MAIN_NETWORK, raw_key),
            Network::Testnet | Network::Regtest => {
                UnifiedIncomingViewingKey::decode(&TEST_NETWORK, raw_key)
            }
        }
        .map_err(ZcashAdapterError::InvalidViewingKey)
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

struct IncomingSaplingScannerKey {
    account_id: u32,
    ivk: SaplingIncomingViewingKey,
}

impl ScanningKeyOps<SaplingDomain, u32, SaplingNullifier> for IncomingSaplingScannerKey {
    fn prepare(&self) -> sapling::note_encryption::PreparedIncomingViewingKey {
        self.ivk.prepare()
    }

    fn account_id(&self) -> &u32 {
        &self.account_id
    }

    fn key_scope(&self) -> Option<zip32::Scope> {
        None
    }

    fn nf(&self, _note: &sapling::Note, _note_position: incrementalmerkletree::Position) -> Option<SaplingNullifier> {
        None
    }
}

struct IncomingOrchardScannerKey {
    account_id: u32,
    ivk: OrchardIncomingViewingKey,
}

impl ScanningKeyOps<OrchardDomain, u32, OrchardNullifier> for IncomingOrchardScannerKey {
    fn prepare(&self) -> orchard::keys::PreparedIncomingViewingKey {
        self.ivk.prepare()
    }

    fn account_id(&self) -> &u32 {
        &self.account_id
    }

    fn key_scope(&self) -> Option<zip32::Scope> {
        None
    }

    fn nf(
        &self,
        _note: &orchard::note::Note,
        _note_position: incrementalmerkletree::Position,
    ) -> Option<OrchardNullifier> {
        None
    }
}
