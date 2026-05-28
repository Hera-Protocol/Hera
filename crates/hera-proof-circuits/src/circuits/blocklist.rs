use ark_bls12_381::Fr;
use ark_ff::{AdditiveGroup, Field};
use ark_poly::{univariate::DensePolynomial, DenseUVPolynomial, Polynomial};
use ark_std::rand::RngCore;
use serde::{Deserialize, Serialize};

use hera_proof_witness::WitnessRecord;

use crate::{
    caulk::{
        commitment::{commit_table, TableCommitment},
        prover::{prove as caulk_prove, CaulkPlusProof},
        srs::CaulkPlusSrs,
        verifier::verify as caulk_verify,
    },
    error::ProofError,
};

/// Public inputs visible to the verifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct BlocklistStatement {
    /// Number of entries in the blocklist.
    pub blocklist_size: usize,
    /// Number of events in the user's table.
    pub event_count: usize,
}

/// The complete blocklist non-membership proof bundle.
#[derive(Debug, Clone)]
pub struct BlocklistProof {
    /// Caulk+ proof showing the user's txid hashes are in their own committed table.
    pub membership_proof: CaulkPlusProof,
    /// The txid table commitment produced during proving.
    pub txid_table_commitment: TableCommitment,
    /// For each user txid_hash, the evaluation of the blocklist vanishing
    /// polynomial at that point. Non-zero values prove non-membership.
    pub blocklist_evals: Vec<Fr>,
    /// The blocklist vanishing polynomial coefficients (public).
    /// The verifier recomputes the evaluations from this.
    pub blocklist_vanishing_coeffs: Vec<Fr>,
}

/// Proves: "none of the user's transaction IDs appear in a given blocklist."
///
/// Non-membership is demonstrated by evaluating the blocklist's vanishing
/// polynomial Z_B(x) at each user txid_hash. If Z_B(txid_hash) != 0 for all
/// txid hashes, then none of them are roots of Z_B, proving non-membership.
///
/// The Caulk+ proof additionally proves that the txid_hashes genuinely come
/// from the user's committed event table.
pub fn prove_no_blocklist_exposure(
    srs: &CaulkPlusSrs,
    records: &[WitnessRecord],
    blocklist: &[Fr],
    rng: &mut impl RngCore,
) -> Result<BlocklistProof, ProofError> {
    if records.is_empty() {
        return Err(ProofError::ProofGeneration("empty event set".into()));
    }

    // Build the txid_hash table from the user's events.
    let txid_table: Vec<Fr> = records.iter().map(|r| r.txid_hash).collect();
    let all_indices: Vec<usize> = (0..records.len()).collect();

    // Commit to the user's txid table.
    let table_commitment = commit_table(srs, &txid_table, rng)?;

    // Generate a Caulk+ proof that the txid hashes belong to the committed table.
    let membership_proof = caulk_prove(srs, &table_commitment, &all_indices, rng)?;

    // Compute the blocklist vanishing polynomial: Z_B(x) = prod(x - b_i).
    let blocklist_vanishing = build_vanishing_polynomial(blocklist);

    // Evaluate Z_B at each user txid_hash. Non-zero means non-membership.
    let blocklist_evals: Vec<Fr> = txid_table
        .iter()
        .map(|txid_hash| blocklist_vanishing.evaluate(txid_hash))
        .collect();

    Ok(BlocklistProof {
        membership_proof,
        txid_table_commitment: table_commitment,
        blocklist_evals,
        blocklist_vanishing_coeffs: blocklist_vanishing.coeffs().to_vec(),
    })
}

/// Verifies a blocklist non-membership proof.
///
/// Checks:
/// 1. The Caulk+ proof is valid (txid hashes are in the committed table).
/// 2. All blocklist evaluations are non-zero (proving non-membership).
/// 3. The evaluations are consistent with the blocklist vanishing polynomial
///    evaluated at the looked-up txid hash values.
pub fn verify_no_blocklist_exposure(
    srs: &CaulkPlusSrs,
    _statement: &BlocklistStatement,
    proof: &BlocklistProof,
) -> Result<bool, ProofError> {
    // Verify the Caulk+ membership proof.
    let caulk_valid = caulk_verify(srs, &proof.txid_table_commitment, &proof.membership_proof)?;
    if !caulk_valid {
        return Ok(false);
    }

    // Check that all blocklist evaluations are non-zero.
    for eval in &proof.blocklist_evals {
        if *eval == Fr::ZERO {
            return Ok(false);
        }
    }

    // Verify that looked-up values and blocklist evaluations have matching counts.
    if proof.membership_proof.looked_up_values.len() != proof.blocklist_evals.len() {
        return Ok(false);
    }

    // Recompute and verify the blocklist evaluations against the looked-up values.
    let vanishing =
        DensePolynomial::from_coefficients_vec(proof.blocklist_vanishing_coeffs.clone());
    for (i, txid_hash) in proof.membership_proof.looked_up_values.iter().enumerate() {
        let expected = vanishing.evaluate(txid_hash);
        if expected != proof.blocklist_evals[i] {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Builds the vanishing polynomial Z_B(x) = product of (x - b_i) for all
/// blocklist entries.
fn build_vanishing_polynomial(blocklist: &[Fr]) -> DensePolynomial<Fr> {
    if blocklist.is_empty() {
        // Empty blocklist: Z_B(x) = 1, so everything is non-member.
        return DensePolynomial::from_coefficients_vec(vec![Fr::ONE]);
    }

    let mut result = DensePolynomial::from_coefficients_vec(vec![Fr::ONE]);
    for &b in blocklist {
        let factor = DensePolynomial::from_coefficients_vec(vec![-b, Fr::ONE]);
        result = poly_mul(&result, &factor);
    }
    result
}

fn poly_mul(a: &DensePolynomial<Fr>, b: &DensePolynomial<Fr>) -> DensePolynomial<Fr> {
    let a_coeffs = a.coeffs();
    let b_coeffs = b.coeffs();
    if a_coeffs.is_empty() || b_coeffs.is_empty() {
        return DensePolynomial::from_coefficients_vec(vec![]);
    }
    let mut result = vec![Fr::ZERO; a_coeffs.len() + b_coeffs.len() - 1];
    for (i, a_coeff) in a_coeffs.iter().enumerate() {
        for (j, b_coeff) in b_coeffs.iter().enumerate() {
            result[i + j] += *a_coeff * b_coeff;
        }
    }
    DensePolynomial::from_coefficients_vec(result)
}

#[cfg(test)]
mod tests {
    use ark_bls12_381::Fr;

    use hera_proof_witness::WitnessRecord;

    use super::*;

    fn make_record(txid_hash_val: u64) -> WitnessRecord {
        WitnessRecord {
            amount: Fr::from(100u64),
            event_type: Fr::from(1u64),
            txid_hash: Fr::from(txid_hash_val),
            block_height: Fr::from(100u64),
            timestamp: Fr::from(1000000u64),
            risk_score: Fr::from(5u64),
        }
    }

    #[test]
    fn non_membership_proof_succeeds_when_no_overlap() {
        let mut rng = ark_std::test_rng();
        let srs = CaulkPlusSrs::generate(64, &mut rng).unwrap();

        let records = vec![make_record(10), make_record(20), make_record(30)];
        let blocklist = vec![Fr::from(100u64), Fr::from(200u64)]; // No overlap

        let statement = BlocklistStatement {
            blocklist_size: 2,
            event_count: 3,
        };

        let proof = prove_no_blocklist_exposure(&srs, &records, &blocklist, &mut rng).unwrap();
        assert!(verify_no_blocklist_exposure(&srs, &statement, &proof).unwrap());
    }

    #[test]
    fn non_membership_proof_fails_when_overlap() {
        let mut rng = ark_std::test_rng();
        let srs = CaulkPlusSrs::generate(64, &mut rng).unwrap();

        let records = vec![make_record(10), make_record(200)]; // 200 is in blocklist
        let blocklist = vec![Fr::from(100u64), Fr::from(200u64)];

        let statement = BlocklistStatement {
            blocklist_size: 2,
            event_count: 2,
        };

        let proof = prove_no_blocklist_exposure(&srs, &records, &blocklist, &mut rng).unwrap();
        assert!(!verify_no_blocklist_exposure(&srs, &statement, &proof).unwrap());
    }
}
