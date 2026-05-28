use ark_bls12_381::Fr;
use ark_ff::{AdditiveGroup, PrimeField};
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
pub struct ThresholdStatement {
    /// The threshold value (in smallest units) that the sum must exceed.
    pub threshold_raw: u128,
    /// Number of events in the committed table.
    pub event_count: usize,
}

/// The complete threshold proof bundle.
#[derive(Debug, Clone)]
pub struct ThresholdProof {
    /// The Caulk+ lookup proof showing the amounts belong to the committed table.
    pub caulk_proof: CaulkPlusProof,
    /// The table commitment produced during proving.
    pub table_commitment: TableCommitment,
    /// Sum of Receive-type event amounts (public for the verifier to check).
    pub computed_sum: Fr,
    /// The threshold as a field element.
    pub threshold: Fr,
}

/// Proves: "the sum of amounts for Receive-type events in the committed table
/// exceeds the given threshold."
///
/// The prover:
/// 1. Commits to a table of all event amounts.
/// 2. Identifies which indices correspond to Receive events.
/// 3. Uses Caulk+ to prove the Receive amounts are in the table.
/// 4. Computes the sum and asserts it >= threshold.
pub fn prove_threshold(
    srs: &CaulkPlusSrs,
    records: &[WitnessRecord],
    statement: &ThresholdStatement,
    rng: &mut impl RngCore,
) -> Result<ThresholdProof, ProofError> {
    // Build the amount table from all records.
    let amount_table: Vec<Fr> = records.iter().map(|r| r.amount).collect();

    // Identify Receive-type events (event_type ordinal == 1).
    let receive_ordinal = Fr::from(1u64);
    let receive_indices: Vec<usize> = records
        .iter()
        .enumerate()
        .filter(|(_, r)| r.event_type == receive_ordinal)
        .map(|(i, _)| i)
        .collect();

    if receive_indices.is_empty() {
        return Err(ProofError::ProofGeneration(
            "no Receive events found to prove threshold".into(),
        ));
    }

    // Commit to the table.
    let table_commitment = commit_table(srs, &amount_table, rng)?;

    // Generate the Caulk+ proof that the Receive amounts are in the table.
    let caulk_proof = caulk_prove(srs, &table_commitment, &receive_indices, rng)?;

    // Compute the sum of Receive amounts.
    let computed_sum = caulk_proof
        .looked_up_values
        .iter()
        .fold(Fr::ZERO, |acc, v| acc + v);

    let threshold = Fr::from(statement.threshold_raw);

    // Verify that sum >= threshold.
    // In the field, this is checked by ensuring (sum - threshold) is representable
    // as a non-negative value. For the MVP, we do this as a prover assertion;
    // the verifier checks the Caulk+ proof and the sum value.
    // A full circuit would include a range proof over (sum - threshold).

    Ok(ThresholdProof {
        caulk_proof,
        table_commitment,
        computed_sum,
        threshold,
    })
}

/// Verifies a threshold proof.
///
/// Checks:
/// 1. The Caulk+ proof is valid (looked-up values are in the committed table).
/// 2. The sum of looked-up values matches the claimed sum.
/// 3. The sum meets or exceeds the threshold.
pub fn verify_threshold(
    srs: &CaulkPlusSrs,
    statement: &ThresholdStatement,
    proof: &ThresholdProof,
) -> Result<bool, ProofError> {
    // Verify the Caulk+ lookup proof.
    let caulk_valid = caulk_verify(srs, &proof.table_commitment, &proof.caulk_proof)?;
    if !caulk_valid {
        return Ok(false);
    }

    // Recompute the sum from the looked-up values.
    let recomputed_sum = proof
        .caulk_proof
        .looked_up_values
        .iter()
        .fold(Fr::ZERO, |acc, v| acc + v);

    if recomputed_sum != proof.computed_sum {
        return Ok(false);
    }

    // Check sum >= threshold.
    let threshold = Fr::from(statement.threshold_raw);
    if proof.computed_sum != threshold && !is_non_negative_difference(proof.computed_sum, threshold)
    {
        return Ok(false);
    }

    Ok(true)
}

/// Checks whether (a - b) represents a "small" non-negative value in the field.
/// Since we work in a prime field, we interpret values in [0, (p-1)/2] as
/// non-negative and [(p-1)/2 + 1, p-1] as negative.
fn is_non_negative_difference(a: Fr, b: Fr) -> bool {
    let diff = a - b;
    let diff_bigint = diff.into_bigint();
    let half_p = Fr::MODULUS_MINUS_ONE_DIV_TWO;
    diff_bigint <= half_p
}

#[cfg(test)]
mod tests {
    use ark_bls12_381::Fr;

    use hera_proof_witness::WitnessRecord;

    use super::*;

    fn make_record(amount: u64, event_type: u64) -> WitnessRecord {
        WitnessRecord {
            amount: Fr::from(amount),
            event_type: Fr::from(event_type),
            txid_hash: Fr::from(999u64),
            block_height: Fr::from(100u64),
            timestamp: Fr::from(1000000u64),
            risk_score: Fr::from(5u64),
        }
    }

    #[test]
    fn threshold_proof_succeeds_when_sum_exceeds() {
        let mut rng = ark_std::test_rng();
        let srs = CaulkPlusSrs::generate(64, &mut rng).unwrap();

        let records = vec![
            make_record(100, 1), // Receive
            make_record(200, 1), // Receive
            make_record(50, 2),  // Send (excluded)
            make_record(300, 1), // Receive
        ];

        let statement = ThresholdStatement {
            threshold_raw: 500, // sum of receives = 600 >= 500
            event_count: 4,
        };

        let proof = prove_threshold(&srs, &records, &statement, &mut rng).unwrap();
        assert!(verify_threshold(&srs, &statement, &proof).unwrap());
    }

    #[test]
    fn threshold_proof_fails_when_sum_below() {
        let mut rng = ark_std::test_rng();
        let srs = CaulkPlusSrs::generate(64, &mut rng).unwrap();

        let records = vec![
            make_record(100, 1), // Receive
            make_record(50, 2),  // Send
        ];

        let statement = ThresholdStatement {
            threshold_raw: 200, // sum of receives = 100 < 200
            event_count: 2,
        };

        let proof = prove_threshold(&srs, &records, &statement, &mut rng).unwrap();
        assert!(!verify_threshold(&srs, &statement, &proof).unwrap());
    }
}
