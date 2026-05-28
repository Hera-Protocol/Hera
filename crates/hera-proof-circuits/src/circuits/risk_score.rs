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
pub struct RiskScoreStatement {
    /// Maximum acceptable risk score [0..100].
    pub max_risk: u8,
    /// Number of events in the committed table.
    pub event_count: usize,
}

/// The complete risk score proof bundle.
#[derive(Debug, Clone)]
pub struct RiskScoreProof {
    /// Caulk+ proof showing the risk scores are in the committed table.
    pub caulk_proof: CaulkPlusProof,
    /// The risk table commitment produced during proving.
    pub risk_table_commitment: TableCommitment,
    /// The maximum risk score found across all events.
    pub observed_max: Fr,
    /// The threshold as a field element.
    pub max_risk: Fr,
}

/// Proves: "the maximum risk score across all events is at or below max_risk."
///
/// The prover:
/// 1. Commits to a table of all risk scores.
/// 2. Uses Caulk+ to prove the scores belong to the committed table.
/// 3. Asserts that every score <= max_risk.
pub fn prove_risk_below(
    srs: &CaulkPlusSrs,
    records: &[WitnessRecord],
    statement: &RiskScoreStatement,
    rng: &mut impl RngCore,
) -> Result<RiskScoreProof, ProofError> {
    if records.is_empty() {
        return Err(ProofError::ProofGeneration("empty event set".into()));
    }

    // Build the risk score table.
    let risk_table: Vec<Fr> = records.iter().map(|r| r.risk_score).collect();
    let all_indices: Vec<usize> = (0..records.len()).collect();

    // Commit to the table.
    let table_commitment = commit_table(srs, &risk_table, rng)?;

    // Generate the Caulk+ proof that the scores are in the committed table.
    let caulk_proof = caulk_prove(srs, &table_commitment, &all_indices, rng)?;

    // Find the observed maximum risk score.
    let max_risk_fr = Fr::from(u64::from(statement.max_risk));
    let observed_max = caulk_proof
        .looked_up_values
        .iter()
        .copied()
        .max_by(|a, b| {
            a.into_bigint()
                .partial_cmp(&b.into_bigint())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(Fr::ZERO);

    Ok(RiskScoreProof {
        caulk_proof,
        risk_table_commitment: table_commitment,
        observed_max,
        max_risk: max_risk_fr,
    })
}

/// Verifies a risk score proof.
///
/// Checks:
/// 1. The Caulk+ proof is valid (risk scores are in the committed table).
/// 2. Every looked-up risk score is <= max_risk.
pub fn verify_risk_below(
    srs: &CaulkPlusSrs,
    statement: &RiskScoreStatement,
    proof: &RiskScoreProof,
) -> Result<bool, ProofError> {
    // Verify the Caulk+ lookup proof.
    let caulk_valid = caulk_verify(srs, &proof.risk_table_commitment, &proof.caulk_proof)?;
    if !caulk_valid {
        return Ok(false);
    }

    // Check every looked-up risk score is <= max_risk.
    let max_risk = Fr::from(u64::from(statement.max_risk));
    for score in &proof.caulk_proof.looked_up_values {
        if !is_at_most(score, &max_risk) {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Checks whether `value <= bound` by verifying that (bound - value) is
/// in the non-negative half of the field [0, (p-1)/2].
fn is_at_most(value: &Fr, bound: &Fr) -> bool {
    if value == bound {
        return true;
    }
    let diff = *bound - *value;
    let diff_bigint = diff.into_bigint();
    let half_p = Fr::MODULUS_MINUS_ONE_DIV_TWO;
    diff_bigint <= half_p
}

#[cfg(test)]
mod tests {
    use ark_bls12_381::Fr;

    use hera_proof_witness::WitnessRecord;

    use super::*;

    fn make_record(risk: u64) -> WitnessRecord {
        WitnessRecord {
            amount: Fr::from(100u64),
            event_type: Fr::from(1u64),
            txid_hash: Fr::from(999u64),
            block_height: Fr::from(100u64),
            timestamp: Fr::from(1000000u64),
            risk_score: Fr::from(risk),
        }
    }

    #[test]
    fn risk_proof_succeeds_when_all_below() {
        let mut rng = ark_std::test_rng();
        let srs = CaulkPlusSrs::generate(64, &mut rng).unwrap();

        let records = vec![make_record(10), make_record(25), make_record(50)];
        let statement = RiskScoreStatement {
            max_risk: 60,
            event_count: 3,
        };

        let proof = prove_risk_below(&srs, &records, &statement, &mut rng).unwrap();
        assert!(verify_risk_below(&srs, &statement, &proof).unwrap());
    }

    #[test]
    fn risk_proof_fails_when_one_exceeds() {
        let mut rng = ark_std::test_rng();
        let srs = CaulkPlusSrs::generate(64, &mut rng).unwrap();

        let records = vec![make_record(10), make_record(80)]; // 80 > 60
        let statement = RiskScoreStatement {
            max_risk: 60,
            event_count: 2,
        };

        let proof = prove_risk_below(&srs, &records, &statement, &mut rng).unwrap();
        assert!(!verify_risk_below(&srs, &statement, &proof).unwrap());
    }
}
