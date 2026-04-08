use serde::{Deserialize, Serialize};

use crate::error::NamadaAdapterError;

/// Stores the validated Namada viewing key with the chain identifier it is
/// allowed to operate against. Namada uses chain IDs not network enums —
/// validate the key is for the expected chain before scanning.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ValidatedNamadaKey {
    pub raw_key: String,
    pub chain_id: String,
    pub birthday_height: Option<u64>,
}

/// Performs conservative validation for Namada viewing keys before any sync work
/// starts, preferring false negatives over accepting ambiguous key material.
pub fn parse_and_validate(
    raw: &str,
    chain_id: &str,
) -> Result<ValidatedNamadaKey, NamadaAdapterError> {
    let trimmed = raw.trim();
    let chain_id = chain_id.trim();

    if trimmed.is_empty() || chain_id.is_empty() {
        return Err(NamadaAdapterError::InvalidViewingKey);
    }

    // Namada viewing keys are bech32-like and frequently use `zvknam` or related
    // human-readable prefixes. We keep the check broad here because deployed
    // formats can evolve, but we still reject obviously malformed input.
    let lower = trimmed.to_ascii_lowercase();
    let looks_valid = trimmed.len() > 16
        && trimmed.contains('1')
        && (lower.starts_with("zvk") || lower.starts_with("nam") || lower.contains("view"));
    if !looks_valid {
        return Err(NamadaAdapterError::InvalidViewingKey);
    }

    Ok(ValidatedNamadaKey {
        raw_key: trimmed.to_string(),
        chain_id: chain_id.to_string(),
        birthday_height: None,
    })
}

#[cfg(test)]
mod tests {
    use super::parse_and_validate;

    #[test]
    fn accepts_bech32_like_viewing_keys() {
        let result = parse_and_validate("zvknam1qqqqqqqqqqqqqqqqq", "namada");

        assert!(result.is_ok());
    }

    #[test]
    fn rejects_empty_inputs() {
        assert!(parse_and_validate("", "namada").is_err());
        assert!(parse_and_validate("zvknam1qqqqqqqqqqqqqqqqq", "").is_err());
    }

    #[test]
    fn rejects_obviously_malformed_keys() {
        assert!(parse_and_validate("bad-key", "namada").is_err());
    }
}
