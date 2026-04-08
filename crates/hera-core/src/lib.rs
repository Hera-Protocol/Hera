#![forbid(unsafe_code)]

pub mod error;
pub mod evidence;
pub mod normalization;
pub mod risk;

pub use error::NormalizationError;
pub use evidence::{build_evidence_refs, EvidenceRef};
pub use normalization::{
    convert_raw_to_decimal, convert_zatoshis_to_decimal, normalize_namada_note,
    normalize_zcash_note, NormalizationContext,
};
pub use risk::{RiskEvaluator, RiskFlag, Severity};
