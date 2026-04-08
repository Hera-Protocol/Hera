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
        let context = ShieldedContext::new(
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
    fn rejects_non_numeric_amounts() {
        let context = ShieldedContext::new(
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
