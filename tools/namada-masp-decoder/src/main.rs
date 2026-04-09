use std::collections::{BTreeMap, HashMap};
use std::io::{Read as _, Write as _};
use std::str::FromStr;

use anyhow::{anyhow, bail, Context as _};
use borsh::BorshDeserialize;
use chrono::{DateTime, Utc};
use hera_namada_adapter::{ExternalDecodeRequest, ExternalDecodeResponse, MaspNote};
use hera_types::Asset;
use masp_primitives::ff::PrimeField;
use masp_primitives::sapling::note_encryption::{
    try_sapling_note_decryption, PreparedIncomingViewingKey,
};
use masp_primitives::transaction::components::OutputDescription;
use masp_primitives::transaction::{Authorization, Authorized};
use namada_core::chain::BlockHeight;
use namada_core::masp::{ExtendedViewingKey, MaspTransaction};
use namada_core::storage::TxIndex;
use namada_sdk::masp::utils::{MaspIndexedTx, MaspTxKind};
use namada_sdk::masp::{NotePosition, NETWORK};
use namada_tx::IndexedTx;
use serde::Deserialize;
use serde_json::Value;

type SaplingProof = OutputDescription<
    <
        <Authorized as Authorization>::SaplingAuth
        as masp_primitives::transaction::components::sapling::Authorization
    >::Proof
>;

/// Reads an external decoder request from stdin, performs real MASP note
/// ownership detection with the official Namada primitives, and writes the
/// resulting owned notes as JSON on stdout. This binary stays isolated from
/// the main Hera workspace so the heavier Namada dependency graph remains
/// outside the core Stage 1 services.
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut buffer = Vec::new();
    stdin
        .read_to_end(&mut buffer)
        .context("failed to read decoder request from stdin")?;

    let request = serde_json::from_slice::<ExternalDecodeRequest>(&buffer)
        .context("failed to parse decoder request json")?;
    let response = decode_request(request).await?;

    serde_json::to_writer(&mut stdout, &response).context("failed to write decoder response")?;
    stdout.flush().context("failed to flush decoder response")?;
    Ok(())
}

async fn decode_request(request: ExternalDecodeRequest) -> anyhow::Result<ExternalDecodeResponse> {
    if request.public_state.tx_count() == 0 {
        return Ok(ExternalDecodeResponse { notes: Vec::new() });
    }

    let viewing_key = parse_viewing_key(&request.key.raw_key)?;
    let snapshot = Snapshot::from_public_state(&request.public_state)?;
    let note_index = parse_note_index(&snapshot.note_index)?;
    let transactions = parse_transactions(snapshot.txs, request.key.birthday_height)?;
    let timestamp_cache = fetch_block_timestamps(unique_block_heights(&transactions)).await?;

    let mut notes = transactions
        .into_iter()
        .map(|tx| decode_notes_from_transaction(&tx, &viewing_key, &note_index, &timestamp_cache))
        .collect::<anyhow::Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    notes.sort_by(|left, right| {
        left.block_height
            .cmp(&right.block_height)
            .then(left.txid.cmp(&right.txid))
            .then(left.note_commitment.cmp(&right.note_commitment))
    });

    Ok(ExternalDecodeResponse { notes })
}

fn parse_viewing_key(raw_key: &str) -> anyhow::Result<masp_primitives::sapling::ViewingKey> {
    let extended = ExtendedViewingKey::from_str(raw_key)
        .map_err(|err| anyhow!("invalid Namada extended viewing key: {err}"))?;
    Ok(extended.as_viewing_key())
}

fn parse_transactions(
    txs: Vec<SnapshotTx>,
    birthday_height: Option<u64>,
) -> anyhow::Result<Vec<DecodedTransaction>> {
    let min_height = birthday_height.unwrap_or(0);
    let mut decoded = Vec::new();

    for tx in txs {
        if tx.block_height < min_height {
            continue;
        }

        for batch in tx.batch {
            let transaction = MaspTransaction::try_from_slice(&batch.bytes).map_err(|err| {
                anyhow!(
                    "failed to deserialize MASP transaction at height {} block index {} batch index {}: {err}",
                    tx.block_height,
                    tx.block_index,
                    batch.masp_tx_index,
                )
            })?;

            let indexed = MaspIndexedTx {
                kind: if batch.is_masp_fee_payment {
                    MaspTxKind::FeePayment
                } else {
                    MaspTxKind::Transfer
                },
                indexed_tx: IndexedTx {
                    block_height: BlockHeight(tx.block_height),
                    block_index: TxIndex::must_from_usize(tx.block_index as usize),
                    batch_index: Some(batch.masp_tx_index as u32),
                },
            };

            decoded.push(DecodedTransaction {
                indexed,
                transaction,
            });
        }
    }

    Ok(decoded)
}

fn parse_note_index(payload: &Value) -> anyhow::Result<BTreeMap<MaspIndexedTx, NotePosition>> {
    let response: NoteIndexResponse = serde_json::from_value(payload.clone())
        .context("failed to parse notes-index payload from public MASP snapshot")?;

    Ok(response
        .notes_index
        .into_iter()
        .map(|note| {
            (
                MaspIndexedTx {
                    indexed_tx: IndexedTx {
                        block_height: BlockHeight(note.block_height),
                        block_index: TxIndex(note.block_index),
                        batch_index: Some(note.batch_index),
                    },
                    kind: if note.is_masp_fee_payment {
                        MaspTxKind::FeePayment
                    } else {
                        MaspTxKind::Transfer
                    },
                },
                note.note_position,
            )
        })
        .collect())
}

fn unique_block_heights(transactions: &[DecodedTransaction]) -> Vec<u64> {
    let mut heights = transactions
        .iter()
        .map(|tx| tx.indexed.indexed_tx.block_height.0)
        .collect::<Vec<_>>();
    heights.sort_unstable();
    heights.dedup();
    heights
}

async fn fetch_block_timestamps(heights: Vec<u64>) -> anyhow::Result<HashMap<u64, DateTime<Utc>>> {
    if heights.is_empty() {
        return Ok(HashMap::new());
    }

    let rpc_url =
        std::env::var("NAMADA_RPC_URL").unwrap_or_else(|_| "https://rpc.namada.net".to_string());
    let client = reqwest::Client::new();
    let mut timestamps = HashMap::new();

    for height in heights {
        let response = client
            .get(format!("{}/block", rpc_url.trim_end_matches('/')))
            .query(&[("height", height.to_string())])
            .send()
            .await
            .with_context(|| {
                format!("failed to query Namada RPC block timestamp for height {height}")
            })?
            .error_for_status()
            .with_context(|| {
                format!("Namada RPC rejected block timestamp lookup for height {height}")
            })?;

        let payload = response.json::<RpcBlockResponse>().await.with_context(|| {
            format!("failed to decode Namada RPC block response for height {height}")
        })?;
        timestamps.insert(height, payload.result.block.header.time);
    }

    Ok(timestamps)
}

fn decode_notes_from_transaction(
    tx: &DecodedTransaction,
    viewing_key: &masp_primitives::sapling::ViewingKey,
    note_index: &BTreeMap<MaspIndexedTx, NotePosition>,
    timestamps: &HashMap<u64, DateTime<Utc>>,
) -> anyhow::Result<Vec<MaspNote>> {
    let Some(_first_note_position) = note_index.get(&tx.indexed).copied() else {
        bail!(
            "notes-index payload did not include the first note position for MASP transaction {}",
            tx.transaction.txid()
        );
    };

    let Some(timestamp) = timestamps
        .get(&tx.indexed.indexed_tx.block_height.0)
        .copied()
    else {
        bail!(
            "missing Namada RPC timestamp for block height {}",
            tx.indexed.indexed_tx.block_height.0
        );
    };

    let prepared = PreparedIncomingViewingKey::new(&viewing_key.ivk());
    let mut notes = Vec::new();
    let outputs = match tx.transaction.sapling_bundle() {
        Some(bundle) => &bundle.shielded_outputs,
        None => return Ok(notes),
    };

    for output in outputs {
        let Some((note, _payment_address, _memo)) =
            try_sapling_note_decryption::<_, SaplingProof>(&NETWORK, 1.into(), &prepared, output)
        else {
            continue;
        };

        let note_commitment = hex::encode(note.cmu().to_repr());
        let asset_id = note.asset_type.to_string();
        notes.push(MaspNote {
            txid: tx.transaction.txid().to_string(),
            block_height: tx.indexed.indexed_tx.block_height.0,
            timestamp,
            asset: Asset {
                // The decoder can always preserve the canonical asset identity
                // from the MASP asset type even when a richer chain lookup is
                // not available in this isolated process.
                symbol: asset_id.clone(),
                asset_id,
                decimals: 0,
            },
            amount_raw: u128::from(note.value),
            note_commitment,
        });
    }

    Ok(notes)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct Snapshot {
    txs: Vec<SnapshotTx>,
    note_index: Value,
}

impl Snapshot {
    fn from_public_state(public_state: &impl serde::Serialize) -> anyhow::Result<Self> {
        let value = serde_json::to_value(public_state)
            .context("failed to serialize public MASP snapshot")?;
        serde_json::from_value(value).context("failed to reshape public MASP snapshot")
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct SnapshotTx {
    block_height: u64,
    block_index: u64,
    batch: Vec<SnapshotBatch>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct SnapshotBatch {
    masp_tx_index: u64,
    is_masp_fee_payment: bool,
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct DecodedTransaction {
    indexed: MaspIndexedTx,
    transaction: MaspTransaction,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct NoteIndexResponse {
    notes_index: Vec<IndexedNotePosition>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct IndexedNotePosition {
    note_position: NotePosition,
    #[serde(rename = "masp_tx_index")]
    batch_index: u32,
    block_index: u32,
    block_height: u64,
    is_masp_fee_payment: bool,
}

#[derive(Debug, Deserialize)]
struct RpcBlockResponse {
    result: RpcBlockResult,
}

#[derive(Debug, Deserialize)]
struct RpcBlockResult {
    block: RpcBlock,
}

#[derive(Debug, Deserialize)]
struct RpcBlock {
    header: RpcBlockHeader,
}

#[derive(Debug, Deserialize)]
struct RpcBlockHeader {
    time: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use hera_namada_adapter::ExternalDecodeRequest;

    use super::decode_request;

    fn sample_request() -> ExternalDecodeRequest {
        serde_json::from_value(serde_json::json!({
            "key": {
                "raw_key": "zvknam1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
                "chain_id": "namada.5f5de2dd1b88cba30586420",
                "birthday_height": 10
            },
            "public_state": {
                "txs": [{
                    "block_height": 42,
                    "block_index": 0,
                    "batch": [{
                        "masp_tx_index": 0,
                        "is_masp_fee_payment": false,
                        "bytes": [1, 2, 3]
                    }]
                }],
                "note_index": {
                    "notes_index": [{
                        "note_position": "1",
                        "masp_tx_index": 0,
                        "block_index": 0,
                        "block_height": 42,
                        "is_masp_fee_payment": false
                    }]
                },
                "witness_map": { "witnesses": [] },
                "commitment_tree": { "commitment_tree": [] }
            }
        }))
        .unwrap_or_else(|err| panic!("failed to build sample request: {err}"))
    }

    #[tokio::test]
    async fn returns_empty_for_empty_public_windows() {
        let request = serde_json::from_value(serde_json::json!({
            "key": {
                "raw_key": "zvknam1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
                "chain_id": "namada.5f5de2dd1b88cba30586420",
                "birthday_height": 10
            },
            "public_state": {
                "txs": [],
                "note_index": { "notes_index": [] },
                "witness_map": { "witnesses": [] },
                "commitment_tree": { "commitment_tree": [] }
            }
        }))
        .unwrap_or_else(|err| panic!("failed to build empty request: {err}"));

        let response = decode_request(request)
            .await
            .unwrap_or_else(|err| panic!("unexpected empty-window decode failure: {err}"));

        assert!(response.notes.is_empty());
    }

    #[tokio::test]
    async fn rejects_invalid_viewing_keys() {
        let mut request = sample_request();
        request.key.raw_key = "not-a-real-key".into();

        let result = decode_request(request).await;

        assert!(result.is_err());
        let err = result.err().expect("expected invalid viewing key error");
        assert!(err
            .to_string()
            .contains("invalid Namada extended viewing key"));
    }

    #[tokio::test]
    async fn rejects_malformed_masp_transactions() {
        let result = decode_request(sample_request()).await;

        assert!(result.is_err());
    }
}
