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
