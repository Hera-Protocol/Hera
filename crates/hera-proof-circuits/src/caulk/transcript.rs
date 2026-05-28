use ark_bls12_381::Fr;
use ark_ec::pairing::Pairing;
use ark_ff::PrimeField;
use ark_serialize::CanonicalSerialize;
use blake2::{Blake2s256, Digest};


/// Fiat-Shamir transcript using Blake2s for deterministic challenge generation.
///
/// Every interactive round in the Caulk+ protocol is made non-interactive by
/// absorbing commitments and scalars into this transcript, then squeezing
/// challenge scalars from the running hash state.
pub struct Transcript {
    state: Blake2s256,
}

impl Transcript {
    pub fn new(domain_sep: &[u8]) -> Self {
        let mut state = Blake2s256::new();
        state.update(domain_sep);
        Self { state }
    }

    /// Absorbs a serialized group element into the transcript.
    pub fn append_g1(&mut self, label: &[u8], point: &<ark_bls12_381::Bls12_381 as Pairing>::G1Affine) {
        self.state.update(label);
        let mut buf = Vec::new();
        point
            .serialize_compressed(&mut buf)
            .expect("G1 serialization should not fail");
        self.state.update(&buf);
    }

    /// Absorbs a scalar field element into the transcript.
    pub fn append_scalar(&mut self, label: &[u8], scalar: &Fr) {
        self.state.update(label);
        let mut buf = Vec::new();
        scalar
            .serialize_compressed(&mut buf)
            .expect("Fr serialization should not fail");
        self.state.update(&buf);
    }

    /// Squeezes a challenge scalar from the current transcript state.
    /// Uses `from_le_bytes_mod_order` so the output is uniformly distributed
    /// over the scalar field.
    pub fn challenge_scalar(&mut self, label: &[u8]) -> Fr {
        self.state.update(label);
        let digest = self.state.clone().finalize();
        // Re-seed the state so subsequent challenges depend on this one.
        self.state.update(&digest);
        Fr::from_le_bytes_mod_order(&digest)
    }
}
