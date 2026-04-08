use hera_types::{
    CanonicalEvent, ChainId, Counterparty, CounterpartyVisibility, EventMemo, EventProvenance,
    EventType, Network,
};
use uuid::Uuid;

use crate::{
    error::NamadaAdapterError,
    types::{FeeRecord, MaspNote, TransferDirection},
};

/// Maps a MASP note into Hera's canonical schema. Each asset in a multi-asset
/// transfer produces a separate `CanonicalEvent`. Never collapse multi-asset
/// transfers into one event — the compliance consumer needs per-asset granularity.
pub fn map_note_to_canonical(
    note: &MaspNote,
    direction: TransferDirection,
    fee: Option<&FeeRecord>,
    scan_version: &str,
) -> Result<CanonicalEvent, NamadaAdapterError> {
    let event_type = match direction {
        TransferDirection::Shielded => EventType::Shield,
        TransferDirection::Unshielded => EventType::Unshield,
        TransferDirection::IntraShielded => EventType::Send,
    };

    let mut notes = Vec::new();
    if let Some(fee) = fee {
        notes.push(format!(
            "fee recorded separately: {} {}",
            fee.amount_raw, fee.asset.asset_id
        ));
        if let Some(payer) = &fee.payer_transparent {
            notes.push(format!("transparent fee payer: {payer}"));
        }
    }

    Ok(CanonicalEvent {
        event_id: Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!(
                "namada:{}:{}:{}:{}",
                note.txid, note.note_commitment, note.asset.asset_id, scan_version
            )
            .as_bytes(),
        ),
        // The case binding and final network selection belong to the orchestration
        // layer above this mapper. We keep a sentinel here rather than inventing
        // case-specific context inside the adapter.
        case_id: Uuid::nil(),
        chain: ChainId::Namada,
        network: Network::Mainnet,
        event_type,
        txid: note.txid.clone(),
        block_height: note.block_height,
        timestamp: note.timestamp,
        asset: note.asset.clone(),
        // MASP values remain raw integer strings inside the adapter. Decimal
        // normalization happens one layer up where asset precision policy lives.
        amount: note.amount_raw.to_string(),
        counterparty: Counterparty {
            visibility: CounterpartyVisibility::Unknown,
            value: None,
        },
        memo: EventMemo {
            present: false,
            hash: None,
        },
        evidence_refs: vec![format!("indexer:{}:{}", note.txid, note.note_commitment)],
        provenance: EventProvenance {
            source: "namada_masp_indexer".to_string(),
            pool: Some("masp".to_string()),
            scan_version: scan_version.to_string(),
        },
        notes,
    })
}
