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
