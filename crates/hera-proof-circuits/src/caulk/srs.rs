use ark_bls12_381::{Bls12_381, Fr};
use ark_poly::univariate::DensePolynomial;
use ark_poly_commit::{marlin::marlin_pc::MarlinKZG10, PolynomialCommitment};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::RngCore;

use crate::error::ProofError;

/// Type alias for the KZG polynomial commitment scheme on BLS12-381.
pub type KZG = MarlinKZG10<Bls12_381, DensePolynomial<Fr>>;
pub type CommitterKey = <KZG as PolynomialCommitment<Fr, DensePolynomial<Fr>>>::CommitterKey;
pub type VerifierKey = <KZG as PolynomialCommitment<Fr, DensePolynomial<Fr>>>::VerifierKey;

/// Structured Reference String for the Caulk+ protocol.
///
/// Contains universal KZG parameters that support polynomial commitments
/// up to `max_degree`. The SRS is generated once and reused across all
/// proof generation runs.
pub struct CaulkPlusSrs {
    pub ck: CommitterKey,
    pub vk: VerifierKey,
    pub max_degree: usize,
}

impl CaulkPlusSrs {
    /// Generates a new SRS via trusted setup for the given maximum polynomial
    /// degree. In production this would consume ceremony output; for development
    /// and testing we use a seeded RNG.
    pub fn generate(max_degree: usize, rng: &mut impl RngCore) -> Result<Self, ProofError> {
        let srs = KZG::setup(max_degree, None, rng)
            .map_err(|e| ProofError::SrsGeneration(e.to_string()))?;
        let supported_degree = max_degree;
        let (ck, vk) = KZG::trim(
            &srs,
            supported_degree,
            0, // enforced_degree_bounds not needed
            None,
        )
        .map_err(|e| ProofError::SrsGeneration(e.to_string()))?;

        Ok(Self { ck, vk, max_degree })
    }

    /// Generates a small development SRS suitable for tables up to 2^10 entries.
    /// Uses a deterministic seed so dev builds are reproducible. NOT for production.
    pub fn dev_srs() -> Result<Self, ProofError> {
        let mut rng = ark_std::test_rng();
        Self::generate(1024, &mut rng)
    }

    /// Serializes the SRS to bytes for persistent storage.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ProofError> {
        let mut buf = Vec::new();
        (self.max_degree as u64)
            .serialize_compressed(&mut buf)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        self.ck
            .serialize_compressed(&mut buf)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        self.vk
            .serialize_compressed(&mut buf)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        Ok(buf)
    }

    /// Deserializes an SRS from bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProofError> {
        let mut reader = bytes;
        let max_degree = u64::deserialize_compressed(&mut reader)
            .map_err(|e| ProofError::Serialization(e.to_string()))?
            as usize;
        let ck = CommitterKey::deserialize_compressed(&mut reader)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        let vk = VerifierKey::deserialize_compressed(&mut reader)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;

        Ok(Self { ck, vk, max_degree })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srs_round_trip_serialization() {
        let srs = CaulkPlusSrs::generate(16, &mut ark_std::test_rng()).unwrap();
        let bytes = srs.to_bytes().unwrap();
        let restored = CaulkPlusSrs::from_bytes(&bytes).unwrap();
        assert_eq!(restored.max_degree, 16);
    }
}
