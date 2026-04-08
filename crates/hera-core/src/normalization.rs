use hera_namada_adapter::{
    map_note_to_canonical as map_namada_note_to_canonical, FeeRecord, MaspNote, TransferDirection,
};
use hera_types::{CanonicalEvent, ChainId, Network};
use hera_zcash_adapter::{map_note_to_canonical as map_zcash_note_to_canonical, TxMeta, ZcashNote};
use uuid::Uuid;

use crate::{
    error::NormalizationError,
    evidence::{build_evidence_refs, EvidenceRef},
};

/// Carries the case and engine metadata required to turn chain-native records
/// into canonical compliance events without leaking adapter-specific concerns upward.
#[derive(Debug, Clone)]
pub struct NormalizationContext {
    pub case_id: Uuid,
    pub chain: ChainId,
    pub network: Network,
    pub scan_engine_version: String,
}

/// Converts a Zcash note into Hera's canonical schema. This layer is the
/// translation boundary. Above here, nothing is chain-specific. Below here,
/// nothing is canonical.
pub fn normalize_zcash_note(
    note: ZcashNote,
    tx_meta: TxMeta,
    ctx: &NormalizationContext,
) -> Result<CanonicalEvent, NormalizationError> {
    if ctx.chain != ChainId::Zcash {
        return Err(NormalizationError::UnsupportedEventType(
            "zcash note provided with non-zcash normalization context".into(),
        ));
    }

    if tx_meta.txid.trim().is_empty() {
        return Err(NormalizationError::MissingRequiredField("txid".into()));
    }

    // The adapter emits integer zatoshis. Normalization is where that becomes the
    // exact decimal string stored and signed in compliance artifacts.
    let mut event = map_zcash_note_to_canonical(&note, &tx_meta, &ctx.scan_engine_version)
        .map_err(|err| NormalizationError::UnsupportedEventType(err.to_string()))?;
    event.case_id = ctx.case_id;
    event.chain = ctx.chain.clone();
    event.network = ctx.network.clone();
    event.amount = convert_zatoshis_to_decimal(note.amount_zatoshis)?;
    event.evidence_refs = build_evidence_refs(
        ChainId::Zcash,
        &[EvidenceRef::CompactBlock {
            height: note.block_height,
            hash: tx_meta.txid,
        }],
    );

    Ok(event)
}

/// Converts a Namada MASP note into Hera's canonical schema. This layer is the
/// translation boundary. Above here, nothing is chain-specific. Below here,
/// nothing is canonical.
pub fn normalize_namada_note(
    note: MaspNote,
    direction: TransferDirection,
    fee: Option<FeeRecord>,
    ctx: &NormalizationContext,
) -> Result<CanonicalEvent, NormalizationError> {
    if ctx.chain != ChainId::Namada {
        return Err(NormalizationError::UnsupportedEventType(
            "namada note provided with non-namada normalization context".into(),
        ));
    }

    if note.txid.trim().is_empty() {
        return Err(NormalizationError::MissingRequiredField("txid".into()));
    }

    let mut event =
        map_namada_note_to_canonical(&note, direction, fee.as_ref(), &ctx.scan_engine_version)
            .map_err(|err| NormalizationError::UnsupportedEventType(err.to_string()))?;
    event.case_id = ctx.case_id;
    event.chain = ctx.chain.clone();
    event.network = ctx.network.clone();
    event.amount = convert_raw_to_decimal(note.amount_raw, note.asset.decimals)?;
    event.evidence_refs = build_evidence_refs(
        ChainId::Namada,
        &[EvidenceRef::IndexerEntry {
            url: format!("tx/{}", note.txid),
            txid: note.txid,
        }],
    );

    Ok(event)
}

/// Returns a decimal string like `1.25000000`. We use `String` not `f64`
/// because we must preserve exact precision in compliance artifacts.
pub fn convert_zatoshis_to_decimal(zatoshis: u64) -> Result<String, NormalizationError> {
    convert_integer_to_decimal(u128::from(zatoshis), 8)
}

/// Converts raw integer amounts into exact decimal strings. We use `String` not
/// `f64` because compliance artifacts must preserve exact chain precision.
pub fn convert_raw_to_decimal(raw: u128, decimals: u8) -> Result<String, NormalizationError> {
    convert_integer_to_decimal(raw, decimals)
}

fn convert_integer_to_decimal(raw: u128, decimals: u8) -> Result<String, NormalizationError> {
    let scale = 10u128
        .checked_pow(u32::from(decimals))
        .ok_or_else(|| NormalizationError::InvalidAmount("decimal scale overflow".into()))?;

    let integer = raw / scale;
    let fractional = raw % scale;

    if decimals == 0 {
        return Ok(integer.to_string());
    }

    Ok(format!(
        "{integer}.{fractional:0width$}",
        width = usize::from(decimals)
    ))
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use hera_namada_adapter::TransferDirection;
    use hera_types::{Asset, ChainId, Network};

    use super::{
        convert_raw_to_decimal, convert_zatoshis_to_decimal, normalize_namada_note,
        normalize_zcash_note, NormalizationContext,
    };

    fn test_context(chain: ChainId) -> NormalizationContext {
        NormalizationContext {
            case_id: match uuid::Uuid::parse_str("11111111-1111-1111-1111-111111111111") {
                Ok(value) => value,
                Err(err) => panic!("failed to construct test uuid: {err}"),
            },
            chain,
            network: Network::Testnet,
            scan_engine_version: "test-engine".into(),
        }
    }

    fn ts(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> chrono::DateTime<Utc> {
        match Utc.with_ymd_and_hms(year, month, day, hour, min, sec) {
            chrono::LocalResult::Single(value) => value,
            other => panic!("unexpected timestamp result: {other:?}"),
        }
    }

    #[test]
    fn converts_zatoshis_exactly() {
        assert_eq!(
            convert_zatoshis_to_decimal(125_000_000).unwrap(),
            "1.25000000"
        );
        assert_eq!(convert_zatoshis_to_decimal(42).unwrap(), "0.00000042");
    }

    #[test]
    fn converts_raw_integer_amounts_exactly() {
        assert_eq!(convert_raw_to_decimal(123_456u128, 3).unwrap(), "123.456");
        assert_eq!(convert_raw_to_decimal(5u128, 0).unwrap(), "5");
    }

    #[test]
    fn normalizes_zcash_note_into_canonical_event() {
        let note = hera_zcash_adapter::ZcashNote {
            txid: "deadbeef".into(),
            block_height: 42,
            pool: hera_zcash_adapter::Pool::Sapling,
            amount_zatoshis: 125_000_000,
            memo: hera_zcash_adapter::MemoVisibility::Absent,
            nullifier: None,
        };
        let tx_meta = hera_zcash_adapter::TxMeta {
            txid: "deadbeef".into(),
            block_height: 42,
            timestamp: ts(2025, 1, 2, 3, 4, 5),
            network: Network::Testnet,
        };

        let event = normalize_zcash_note(note, tx_meta, &test_context(ChainId::Zcash)).unwrap();

        assert_eq!(event.amount, "1.25000000");
        assert_eq!(
            event.case_id.to_string(),
            "11111111-1111-1111-1111-111111111111"
        );
        assert_eq!(
            event.evidence_refs,
            vec!["compactblock:42:deadbeef".to_string()]
        );
    }

    #[test]
    fn normalizes_namada_note_into_canonical_event() {
        let note = hera_namada_adapter::MaspNote {
            txid: "nam-tx-1".into(),
            block_height: 99,
            timestamp: ts(2025, 2, 3, 4, 5, 6),
            asset: Asset {
                symbol: "NAM".into(),
                asset_id: "nam".into(),
                decimals: 6,
            },
            amount_raw: 1_250_000,
            note_commitment: "commitment-1".into(),
        };

        let event = normalize_namada_note(
            note,
            TransferDirection::Shielded,
            None,
            &test_context(ChainId::Namada),
        )
        .unwrap();

        assert_eq!(event.amount, "1.250000");
        assert_eq!(
            event.evidence_refs,
            vec!["indexer:tx/nam-tx-1:nam-tx-1".to_string()]
        );
        assert_eq!(event.network, Network::Testnet);
    }
}
