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
    if let Some(public_state) = context.public_state() {
        if public_state.tx_count() == 0 {
            return Ok(Vec::new());
        }

        let block_heights = public_state.block_heights();

        if !public_state.has_auxiliary_state() {
            return Err(NamadaAdapterError::PublicMaspDecodingUnavailable(
                format!(
                    "public indexer payload is missing auxiliary MASP state; fetched transactions across heights {:?} with {}",
                    block_heights,
                    public_state.summary(),
                ),
            ));
        }

        return Err(NamadaAdapterError::PublicMaspDecodingUnavailable(
            format!(
                "public MASP transaction decoding requires the official Namada MASP decoder; fetched transactions across heights {:?} with {}",
                block_heights,
                public_state.summary(),
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
    use serde_json::json;

    use crate::{
        detect_owned_notes,
        masp_sync::{PublicIndexerState, ShieldedContext, ShieldedEntry},
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
            PublicIndexerState::new(
                vec![crate::masp_sync::IndexedMaspTx {
                    block_height: 7,
                    block_index: 0,
                    batch: vec![crate::masp_sync::IndexedMaspBatchItem {
                        masp_tx_index: 0,
                        is_masp_fee_payment: false,
                        bytes: vec![1, 2, 3],
                    }],
                }],
                json!([{ "note_pos": 1 }]),
                json!({ "path": ["a"] }),
                json!([{ "root": "b" }]),
            ),
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
        assert!(err.to_string().contains("official Namada MASP decoder"));
    }

    #[test]
    fn returns_empty_when_public_window_has_no_transactions() {
        let context = ShieldedContext::new_public(
            PublicIndexerState::new(Vec::new(), json!([]), json!({}), json!([])),
            9,
            9,
        );
        let key = ValidatedNamadaKey {
            raw_key: "zvknam1exampleexample".into(),
            chain_id: "namada".into(),
            birthday_height: Some(1),
        };

        let result = detect_owned_notes(&context, &key);

        assert!(result.is_ok());
        assert!(result.unwrap_or_default().is_empty());
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
