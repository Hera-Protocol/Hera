use ark_bls12_381::{Bls12_381, Fr};
use ark_ec::pairing::Pairing;
use ark_ff::{AdditiveGroup, Field};
use ark_poly::{
    univariate::DensePolynomial, DenseUVPolynomial, EvaluationDomain, GeneralEvaluationDomain,
    Polynomial,
};
use ark_poly_commit::{LabeledPolynomial, PolynomialCommitment};
use ark_std::rand::RngCore;
use ark_std::Zero;

use super::srs::{CaulkPlusSrs, KZG};
use crate::error::ProofError;

/// Represents a committed table — the polynomial commitment to the table
/// values interpolated over roots of unity, plus cached metadata needed
/// by the prover.
#[derive(Clone, Debug)]
pub struct TableCommitment {
    /// The KZG commitment to the table polynomial.
    pub commitment: <Bls12_381 as Pairing>::G1Affine,
    /// The table polynomial in coefficient form.
    pub poly: DensePolynomial<Fr>,
    /// The evaluation domain (roots of unity) used for interpolation.
    pub domain: GeneralEvaluationDomain<Fr>,
    /// Number of table entries.
    pub size: usize,
}

/// Commits to a vector of field elements by interpolating them as a polynomial
/// over roots of unity, then computing the KZG commitment.
///
/// The table size is padded to the next power of two to match the FFT domain.
pub fn commit_table(
    srs: &CaulkPlusSrs,
    table: &[Fr],
    rng: &mut impl RngCore,
) -> Result<TableCommitment, ProofError> {
    if table.is_empty() {
        return Err(ProofError::Commitment("empty table".into()));
    }

    let domain = GeneralEvaluationDomain::<Fr>::new(table.len())
        .ok_or_else(|| ProofError::Commitment("failed to construct FFT domain".into()))?;

    // Pad table to domain size with zeros.
    let mut padded = table.to_vec();
    padded.resize(domain.size(), Fr::ZERO);

    // IFFT: evaluations at roots of unity -> coefficient form.
    let poly = DensePolynomial::from_coefficients_vec(domain.ifft(&padded));

    let labeled = LabeledPolynomial::new("table".to_string(), poly.clone(), None, None);

    let (commitments, _) = KZG::commit(&srs.ck, [&labeled], Some(rng))
        .map_err(|e| ProofError::Commitment(e.to_string()))?;

    let commitment = commitments[0].commitment().comm.0;

    Ok(TableCommitment {
        commitment,
        poly,
        domain,
        size: table.len(),
    })
}

/// Builds the vanishing polynomial Z_I(x) = product of (x - omega^i) for
/// the given indices into the evaluation domain. Used by the Caulk+ prover
/// to isolate the lookup positions.
pub fn vanishing_poly_for_indices(
    domain: &GeneralEvaluationDomain<Fr>,
    indices: &[usize],
) -> DensePolynomial<Fr> {
    let mut result = DensePolynomial::from_coefficients_vec(vec![Fr::ONE]);
    for &idx in indices {
        let omega_i = domain.element(idx);
        // Multiply by (x - omega^i)
        let factor = DensePolynomial::from_coefficients_vec(vec![-omega_i, Fr::ONE]);
        result = poly_mul(&result, &factor);
    }
    result
}

/// Evaluates the table polynomial at the lookup positions and returns
/// the values. Used to extract the looked-up values for the prover.
pub fn extract_table_values(table_commitment: &TableCommitment, indices: &[usize]) -> Vec<Fr> {
    indices
        .iter()
        .map(|&idx| {
            let omega_i = table_commitment.domain.element(idx);
            table_commitment.poly.evaluate(&omega_i)
        })
        .collect()
}

/// Naive polynomial multiplication. Fine for small polynomials (lookup size m).
fn poly_mul(a: &DensePolynomial<Fr>, b: &DensePolynomial<Fr>) -> DensePolynomial<Fr> {
    if a.is_zero() || b.is_zero() {
        return DensePolynomial::from_coefficients_vec(vec![]);
    }
    let a_coeffs = a.coeffs();
    let b_coeffs = b.coeffs();
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
    use super::*;

    #[test]
    fn commits_to_small_table() {
        let srs = CaulkPlusSrs::generate(32, &mut ark_std::test_rng()).unwrap();
        let table = vec![
            Fr::from(10u64),
            Fr::from(20u64),
            Fr::from(30u64),
            Fr::from(40u64),
        ];
        let tc = commit_table(&srs, &table, &mut ark_std::test_rng()).unwrap();
        assert_eq!(tc.size, 4);
    }

    #[test]
    fn extracts_correct_values() {
        let srs = CaulkPlusSrs::generate(32, &mut ark_std::test_rng()).unwrap();
        let table = vec![
            Fr::from(10u64),
            Fr::from(20u64),
            Fr::from(30u64),
            Fr::from(40u64),
        ];
        let tc = commit_table(&srs, &table, &mut ark_std::test_rng()).unwrap();
        let values = extract_table_values(&tc, &[0, 2]);
        assert_eq!(values[0], Fr::from(10u64));
        assert_eq!(values[1], Fr::from(30u64));
    }

    #[test]
    fn vanishing_poly_has_correct_roots() {
        let domain = GeneralEvaluationDomain::<Fr>::new(8).unwrap();
        let indices = vec![1, 3];
        let z = vanishing_poly_for_indices(&domain, &indices);
        for &idx in &indices {
            let omega_i = domain.element(idx);
            assert_eq!(z.evaluate(&omega_i), Fr::ZERO);
        }
        // A non-selected index should not be a root.
        let omega_0 = domain.element(0);
        assert_ne!(z.evaluate(&omega_0), Fr::ZERO);
    }
}
