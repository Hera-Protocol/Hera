use chrono::{DateTime, Utc};
use hera_types::{
    Asset, CanonicalEvent, ChainId, Counterparty, CounterpartyVisibility, EventMemo,
    EventProvenance, EventType, Network,
};
use uuid::Uuid;

use crate::{
    error::ZcashAdapterError,
    types::{MemoVisibility, Pool, ZcashNote},
};

/// Carries the transaction metadata the compact note alone does not include. The
/// network is included here so the mapper does not have to guess chain context.
#[derive(Debug, Clone)]
pub struct TxMeta {
    pub txid: String,
    pub block_height: u32,
    pub timestamp: DateTime<Utc>,
    pub network: Network,
}

/// Maps a chain-specific owned note into Hera's canonical schema. The counterparty
/// for shielded receives is always Unknown unless a memo explicitly identifies them.
/// Do not guess.
pub fn map_note_to_canonical(
    note: &ZcashNote,
    tx_meta: &TxMeta,
    scan_version: &str,
) -> Result<CanonicalEvent, ZcashAdapterError> {
    let pool_label = match note.pool {
        Pool::Orchard => Some("orchard".to_string()),
        Pool::Sapling => Some("sapling".to_string()),
        Pool::Transparent => Some("transparent".to_string()),
    };

    let mut notes = Vec::new();
    let memo = match &note.memo {
        MemoVisibility::Present { hash } => EventMemo {
            present: true,
            hash: Some(hash.clone()),
        },
        MemoVisibility::Absent => EventMemo {
            present: false,
            hash: None,
        },
        MemoVisibility::Encrypted => {
            notes.push("memo was present but not decryptable during scan".to_string());
            EventMemo {
                present: true,
                hash: None,
            }
        }
    };

    Ok(CanonicalEvent {
        event_id: Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!(
                "zcash:{}:{}:{}",
                tx_meta.txid, note.block_height, scan_version
            )
            .as_bytes(),
        ),
        // The case binding happens one layer up in orchestration/normalization.
        // We keep a sentinel here so the adapter can still produce a witness-ready
        // canonical shape without inventing case context it does not own.
        case_id: Uuid::nil(),
        chain: ChainId::Zcash,
        network: tx_meta.network.clone(),
        event_type: EventType::Receive,
        txid: tx_meta.txid.clone(),
        block_height: u64::from(tx_meta.block_height),
        timestamp: tx_meta.timestamp,
        asset: Asset {
            symbol: "ZEC".to_string(),
            asset_id: "zec".to_string(),
            decimals: 8,
        },
        // Amounts remain integer-string encoded in the adapter; exact decimal
        // rendering belongs in the normalization layer above this boundary.
        amount: note.amount_zatoshis.to_string(),
        counterparty: Counterparty {
            visibility: CounterpartyVisibility::Unknown,
            value: None,
        },
        memo,
        evidence_refs: vec![format!(
            "compactblock:{}:{}",
            tx_meta.block_height, tx_meta.txid
        )],
        provenance: EventProvenance {
            source: "lightwalletd".to_string(),
            pool: pool_label,
            scan_version: scan_version.to_string(),
        },
        notes,
    })
}
