use ark_bls12_381::Fr;
use ark_ff::PrimeField;
use blake2::{Blake2s256, Digest};

use crate::error::WitnessError;

/// Domain separation tag for txid hashing. This ensures Hera txid hashes
/// cannot collide with hashes produced by other protocols.
const DOMAIN_SEP: &[u8] = b"hera-txid-v1";

/// Hashes a hex-encoded transaction ID into a single BLS12-381 scalar field
/// element using Blake2s with domain separation.
///
/// The hash output (32 bytes) is reduced modulo the BLS12-381 scalar field
/// order via `Fr::from_le_bytes_mod_order`.
pub fn hash_txid(txid_hex: &str) -> Result<Fr, WitnessError> {
    if txid_hex.trim().is_empty() {
        return Err(WitnessError::InvalidTxid("empty txid".into()));
    }

    let mut hasher = Blake2s256::new();
    hasher.update(DOMAIN_SEP);
    hasher.update(txid_hex.as_bytes());
    let digest = hasher.finalize();

    Ok(Fr::from_le_bytes_mod_order(&digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_hash() {
        let a = hash_txid("deadbeef").unwrap();
        let b = hash_txid("deadbeef").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn different_txids_produce_different_hashes() {
        let a = hash_txid("deadbeef").unwrap();
        let b = hash_txid("cafebabe").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn rejects_empty_txid() {
        assert!(hash_txid("").is_err());
        assert!(hash_txid("  ").is_err());
    }
}
