use bech32::decode;
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
    let (trimmed, birthday_height) = split_birthday_suffix(raw.trim())?;
    let chain_id = chain_id.trim();

    if trimmed.is_empty() || chain_id.is_empty() {
        return Err(NamadaAdapterError::InvalidViewingKey);
    }

    if !looks_like_chain_id(chain_id) {
        return Err(NamadaAdapterError::InvalidViewingKey);
    }

    // Namada viewing keys are bech32-encoded, so we validate the checksum and
    // the human-readable prefix instead of trusting loose string heuristics.
    let (hrp, data) = decode(trimmed).map_err(|_| NamadaAdapterError::InvalidViewingKey)?;
    let hrp = hrp.to_string().to_ascii_lowercase();
    let looks_like_viewing_key = matches!(hrp.as_str(), "zvknam" | "ivknam" | "fvknam")
        || (hrp.ends_with("nam")
            && (hrp.starts_with("zvk") || hrp.starts_with("ivk") || hrp.starts_with("fvk")));
    if !looks_like_viewing_key || data.len() < 16 {
        return Err(NamadaAdapterError::InvalidViewingKey);
    }

    Ok(ValidatedNamadaKey {
        raw_key: trimmed.to_string(),
        chain_id: chain_id.to_string(),
        birthday_height,
    })
}

fn split_birthday_suffix(raw: &str) -> Result<(&str, Option<u64>), NamadaAdapterError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok((trimmed, None));
    }

    let Some((key, suffix)) = trimmed.split_once("<<") else {
        return Ok((trimmed, None));
    };
    if suffix.contains("<<") {
        return Err(NamadaAdapterError::InvalidViewingKey);
    }

    let birthday_height = suffix
        .trim()
        .parse::<u64>()
        .map_err(|_| NamadaAdapterError::InvalidViewingKey)?;
    Ok((key.trim(), Some(birthday_height)))
}

fn looks_like_chain_id(chain_id: &str) -> bool {
    chain_id.len() > 8
        && chain_id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '.' | '-'))
}

#[cfg(test)]
mod tests {
    use bech32::{encode, Bech32, Hrp};

    use super::parse_and_validate;

    fn sample_viewing_key() -> String {
        let hrp = Hrp::parse("zvknam").unwrap_or_else(|err| panic!("invalid test hrp: {err}"));
        let payload = [7u8; 32];
        encode::<Bech32>(hrp, &payload)
            .unwrap_or_else(|err| panic!("failed to build test viewing key: {err}"))
    }

    #[test]
    fn accepts_bech32_viewing_keys() {
        let result = parse_and_validate(&sample_viewing_key(), "namada.5f5de2dd1b88cba30586420");

        assert!(result.is_ok());
    }

    #[test]
    fn rejects_empty_inputs() {
        assert!(parse_and_validate("", "namada.5f5de2dd1b88cba30586420").is_err());
        assert!(parse_and_validate(&sample_viewing_key(), "").is_err());
    }

    #[test]
    fn rejects_obviously_malformed_keys() {
        assert!(parse_and_validate("bad-key", "namada.5f5de2dd1b88cba30586420").is_err());
    }

    #[test]
    fn extracts_birthday_suffix() {
        let result = parse_and_validate(
            &format!("{}<<12345", sample_viewing_key()),
            "namada.5f5de2dd1b88cba30586420",
        );

        assert!(result.is_ok());
        let validated = result.unwrap_or_else(|err| panic!("unexpected parse failure: {err}"));
        assert_eq!(validated.birthday_height, Some(12_345));
    }

    #[test]
    fn rejects_invalid_chain_ids() {
        assert!(parse_and_validate(&sample_viewing_key(), "Namada Mainnet").is_err());
    }
}
