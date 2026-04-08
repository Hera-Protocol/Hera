use hera_types::Asset;

use crate::{
    error::NamadaAdapterError, masp_sync::ShieldedContext, types::MaspNote,
    viewing_key::ValidatedNamadaKey,
};

/// Detection iterates the shielded context and identifies notes decryptable with
/// our viewing key. Each note has an asset type — we must preserve this and
/// never aggregate across assets.
pub fn detect_owned_notes(
    context: &ShieldedContext,
    _key: &ValidatedNamadaKey,
) -> Result<Vec<MaspNote>, NamadaAdapterError> {
    if !context.txs().is_empty() {
        let tx_count = context.txs().len();
        let block_heights = context
            .txs()
            .iter()
            .map(|tx| tx.block_height)
            .collect::<Vec<_>>();
        let block_index_sum = context
            .txs()
            .iter()
            .map(|tx| usize::try_from(tx.block_index).unwrap_or(0usize))
            .sum::<usize>();
        let batch_count = context.txs().iter().map(|tx| tx.batch.len()).sum::<usize>();
        let fee_batch_count = context
            .txs()
            .iter()
            .flat_map(|tx| tx.batch.iter())
            .filter(|batch| batch.is_masp_fee_payment)
            .count();
        let total_byte_len = context
            .txs()
            .iter()
            .flat_map(|tx| tx.batch.iter())
            .map(|batch| usize::try_from(batch.masp_tx_index).unwrap_or(0usize) + batch.bytes.len())
            .sum::<usize>();
        return Err(NamadaAdapterError::MaspSyncFailed(
            format!(
                "public MASP transaction decoding is not implemented yet; fetched {tx_count} indexed transactions across heights {:?}, block-index sum {block_index_sum}, {batch_count} MASP batches, {fee_batch_count} fee batches, and {total_byte_len} raw bytes from the public indexer",
                block_heights
            ),
        ));
    }

    context
        .entries()
        .iter()
        .map(|entry| {
            let amount_raw = entry.amount_raw.parse::<u128>().map_err(|_| {
                NamadaAdapterError::MaspSyncFailed(
                    "indexer returned a non-numeric MASP amount".into(),
                )
            })?;

            Ok(MaspNote {
                txid: entry.txid.clone(),
                block_height: entry.block_height,
                timestamp: entry.timestamp,
                asset: Asset {
                    symbol: entry
                        .asset_symbol
                        .clone()
                        .unwrap_or_else(|| entry.asset_id.clone()),
                    asset_id: entry.asset_id.clone(),
                    decimals: entry.asset_decimals.unwrap_or(6),
                },
                amount_raw,
                note_commitment: entry.note_commitment.clone(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use crate::{
        detect_owned_notes,
        masp_sync::{ShieldedContext, ShieldedEntry},
        viewing_key::ValidatedNamadaKey,
    };

    fn ts(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> chrono::DateTime<Utc> {
        match Utc.with_ymd_and_hms(year, month, day, hour, min, sec) {
            chrono::LocalResult::Single(value) => value,
            other => panic!("unexpected timestamp result: {other:?}"),
        }
    }

    #[test]
    fn detects_owned_notes_from_indexer_entries() {
        let context = ShieldedContext::new_legacy(
            vec![ShieldedEntry {
                txid: "tx-1".into(),
                block_height: 7,
                timestamp: ts(2025, 3, 4, 5, 6, 7),
                asset_id: "ibc/asset-1".into(),
                asset_symbol: Some("ATOM".into()),
                asset_decimals: Some(6),
                amount_raw: "4200000".into(),
                note_commitment: "commitment-1".into(),
            }],
            3,
            7,
        );
        let key = ValidatedNamadaKey {
            raw_key: "zvknam1exampleexample".into(),
            chain_id: "namada".into(),
            birthday_height: Some(1),
        };

        let result = detect_owned_notes(&context, &key);

        assert!(result.is_ok());
        let notes = match result {
            Ok(value) => value,
            Err(err) => panic!("unexpected detection error: {err}"),
        };
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].asset.symbol, "ATOM");
        assert_eq!(notes[0].amount_raw, 4_200_000);
        assert_eq!(notes[0].timestamp, ts(2025, 3, 4, 5, 6, 7));
    }

    #[test]
    fn rejects_public_indexer_batches_until_raw_tx_decoding_lands() {
        let context = ShieldedContext::new_public(
            vec![crate::masp_sync::IndexedMaspTx {
                block_height: 7,
                block_index: 0,
                batch: vec![crate::masp_sync::IndexedMaspBatchItem {
                    masp_tx_index: 0,
                    is_masp_fee_payment: false,
                    bytes: vec![1, 2, 3],
                }],
            }],
            7,
            7,
        );
        let key = ValidatedNamadaKey {
            raw_key: "zvknam1exampleexample".into(),
            chain_id: "namada".into(),
            birthday_height: Some(1),
        };

        let result = detect_owned_notes(&context, &key);

        assert!(result.is_err());
        let err = result.err().expect("expected detection error");
        assert!(err.to_string().contains("public MASP transaction decoding"));
    }

    #[test]
    fn rejects_non_numeric_amounts() {
        let context = ShieldedContext::new_legacy(
            vec![ShieldedEntry {
                txid: "tx-1".into(),
                block_height: 7,
                timestamp: ts(2025, 3, 4, 5, 6, 7),
                asset_id: "nam".into(),
                asset_symbol: Some("NAM".into()),
                asset_decimals: Some(6),
                amount_raw: "not-a-number".into(),
                note_commitment: "commitment-1".into(),
            }],
            3,
            7,
        );
        let key = ValidatedNamadaKey {
            raw_key: "zvknam1exampleexample".into(),
            chain_id: "namada".into(),
            birthday_height: None,
        };

        assert!(detect_owned_notes(&context, &key).is_err());
    }
}
