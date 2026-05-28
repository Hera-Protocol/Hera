use ark_bls12_381::Fr;

use crate::error::WitnessError;

/// Parses a decimal string amount with the given number of decimal places
/// into the smallest-unit integer, then converts to a BLS12-381 scalar.
///
/// Example: `parse_amount("1.25000000", 8)` -> Fr representing 125_000_000.
pub fn parse_amount(amount_str: &str, decimals: u8) -> Result<Fr, WitnessError> {
    let raw = parse_to_smallest_unit(amount_str, decimals)?;
    Ok(Fr::from(raw))
}

/// Returns the smallest-unit integer representation of a decimal string.
pub fn parse_to_smallest_unit(amount_str: &str, decimals: u8) -> Result<u128, WitnessError> {
    let parts: Vec<&str> = amount_str.split('.').collect();
    if parts.len() > 2 {
        return Err(WitnessError::InvalidAmount(format!(
            "multiple decimal points in '{amount_str}'"
        )));
    }

    let integer_part: u128 = parts[0]
        .parse()
        .map_err(|_| WitnessError::InvalidAmount(format!("invalid integer part in '{amount_str}'")))?;

    let fractional_raw = parts.get(1).copied().unwrap_or("");

    if fractional_raw.len() > usize::from(decimals) {
        return Err(WitnessError::InvalidAmount(format!(
            "fractional part exceeds {decimals} decimals in '{amount_str}'"
        )));
    }

    // Pad fractional part to the expected number of decimal places.
    let fractional: u128 = if decimals == 0 {
        0
    } else {
        let padded = format!("{fractional_raw:0<width$}", width = usize::from(decimals));
        padded
            .parse()
            .map_err(|_| WitnessError::InvalidAmount(format!("invalid fractional part in '{amount_str}'")))?
    };

    let scale = 10u128
        .checked_pow(u32::from(decimals))
        .ok_or_else(|| WitnessError::FieldOverflow("decimal scale overflow".into()))?;

    let total = integer_part
        .checked_mul(scale)
        .and_then(|v| v.checked_add(fractional))
        .ok_or_else(|| WitnessError::FieldOverflow(format!("amount overflow for '{amount_str}'")))?;

    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_zcash_amount() {
        assert_eq!(parse_to_smallest_unit("1.25000000", 8).unwrap(), 125_000_000);
    }

    #[test]
    fn parses_zero() {
        assert_eq!(parse_to_smallest_unit("0.00000000", 8).unwrap(), 0);
    }

    #[test]
    fn parses_integer_only() {
        assert_eq!(parse_to_smallest_unit("42", 0).unwrap(), 42);
    }

    #[test]
    fn parses_namada_six_decimals() {
        assert_eq!(parse_to_smallest_unit("1.250000", 6).unwrap(), 1_250_000);
    }

    #[test]
    fn rejects_too_many_decimals() {
        assert!(parse_to_smallest_unit("1.123456789", 8).is_err());
    }

    #[test]
    fn parse_amount_returns_fr() {
        let fr = parse_amount("1.25000000", 8).unwrap();
        assert_eq!(fr, Fr::from(125_000_000u128));
    }
}
