use ark_bls12_381::{Bls12_381, Fr};
use ark_ec::pairing::Pairing;
use ark_ff::{AdditiveGroup, Field};
use ark_poly::{univariate::DensePolynomial, DenseUVPolynomial, EvaluationDomain, Polynomial};
use ark_poly_commit::{LabeledPolynomial, PolynomialCommitment};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::RngCore;

use super::{
    commitment::{vanishing_poly_for_indices, TableCommitment},
    srs::{CaulkPlusSrs, KZG},
    transcript::Transcript,
};
use crate::error::ProofError;

/// A Caulk+ proof demonstrating that a set of looked-up values belong
/// to a committed table, without revealing which positions were accessed.
///
/// The proof consists of:
/// - Commitments to the subset polynomial and its vanishing polynomial
/// - KZG opening proofs at a random evaluation point
/// - Evaluated values at the challenge point
#[derive(Clone, Debug)]
pub struct CaulkPlusProof {
    /// KZG commitment to the subset interpolation polynomial c_I.
    pub c_i: <Bls12_381 as Pairing>::G1Affine,
    /// KZG commitment to the vanishing polynomial z_I over lookup positions.
    pub c_z_i: <Bls12_381 as Pairing>::G1Affine,
    /// Opening proof for the table polynomial at the challenge point.
    pub pi_table: <Bls12_381 as Pairing>::G1Affine,
    /// Opening proof for the subset polynomial at the challenge point.
    pub pi_subset: <Bls12_381 as Pairing>::G1Affine,
    /// Table polynomial evaluated at the challenge point.
    pub table_eval: Fr,
    /// Subset polynomial evaluated at the challenge point.
    pub subset_eval: Fr,
    /// Vanishing polynomial evaluated at the challenge point.
    pub z_i_eval: Fr,
    /// The looked-up values (public to the verifier in our usage).
    pub looked_up_values: Vec<Fr>,
}

impl CaulkPlusProof {
    pub fn to_bytes(&self) -> Result<Vec<u8>, ProofError> {
        let mut buf = Vec::new();
        self.c_i
            .serialize_compressed(&mut buf)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        self.c_z_i
            .serialize_compressed(&mut buf)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        self.pi_table
            .serialize_compressed(&mut buf)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        self.pi_subset
            .serialize_compressed(&mut buf)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        self.table_eval
            .serialize_compressed(&mut buf)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        self.subset_eval
            .serialize_compressed(&mut buf)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        self.z_i_eval
            .serialize_compressed(&mut buf)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        (self.looked_up_values.len() as u64)
            .serialize_compressed(&mut buf)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        for v in &self.looked_up_values {
            v.serialize_compressed(&mut buf)
                .map_err(|e| ProofError::Serialization(e.to_string()))?;
        }
        Ok(buf)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProofError> {
        let mut reader = &bytes[..];
        let c_i = <Bls12_381 as Pairing>::G1Affine::deserialize_compressed(&mut reader)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        let c_z_i = <Bls12_381 as Pairing>::G1Affine::deserialize_compressed(&mut reader)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        let pi_table = <Bls12_381 as Pairing>::G1Affine::deserialize_compressed(&mut reader)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        let pi_subset = <Bls12_381 as Pairing>::G1Affine::deserialize_compressed(&mut reader)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        let table_eval = Fr::deserialize_compressed(&mut reader)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        let subset_eval = Fr::deserialize_compressed(&mut reader)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        let z_i_eval = Fr::deserialize_compressed(&mut reader)
            .map_err(|e| ProofError::Serialization(e.to_string()))?;
        let count = u64::deserialize_compressed(&mut reader)
            .map_err(|e| ProofError::Serialization(e.to_string()))? as usize;
        let mut looked_up_values = Vec::with_capacity(count);
        for _ in 0..count {
            looked_up_values.push(
                Fr::deserialize_compressed(&mut reader)
                    .map_err(|e| ProofError::Serialization(e.to_string()))?,
            );
        }
        Ok(Self {
            c_i,
            c_z_i,
            pi_table,
            pi_subset,
            table_eval,
            subset_eval,
            z_i_eval,
            looked_up_values,
        })
    }
}

/// Generates a Caulk+ proof that the values at `lookup_indices` in the
/// committed table match the expected looked-up values.
///
/// Protocol outline:
/// 1. Interpolate a subset polynomial p_I over the looked-up values.
/// 2. Compute the vanishing polynomial z_I over the lookup positions.
/// 3. Commit to both polynomials.
/// 4. Derive a challenge point alpha via Fiat-Shamir.
/// 5. Open the table polynomial, subset polynomial at alpha.
/// 6. The verifier checks that the relationship holds at alpha.
pub fn prove(
    srs: &CaulkPlusSrs,
    table_commitment: &TableCommitment,
    lookup_indices: &[usize],
    rng: &mut impl RngCore,
) -> Result<CaulkPlusProof, ProofError> {
    if lookup_indices.is_empty() {
        return Err(ProofError::ProofGeneration("empty lookup set".into()));
    }

    let domain = &table_commitment.domain;

    // Step 1: Extract the looked-up values from the table.
    let looked_up_values: Vec<Fr> = lookup_indices
        .iter()
        .map(|&idx| {
            let omega_i = domain.element(idx);
            table_commitment.poly.evaluate(&omega_i)
        })
        .collect();

    // Step 2: Interpolate a subset polynomial over the lookup positions.
    // p_I(omega^i) = table[i] for each i in lookup_indices.
    // Use Lagrange interpolation over the actual table-domain points.
    let interp_points: Vec<Fr> = lookup_indices
        .iter()
        .map(|&idx| domain.element(idx))
        .collect();
    let subset_poly = lagrange_interpolate(&interp_points, &looked_up_values);

    // Step 3: Compute the vanishing polynomial z_I(x) = prod(x - omega^i).
    let z_i_poly = vanishing_poly_for_indices(domain, lookup_indices);

    // Step 4: Commit to subset and vanishing polynomials.
    let labeled_subset =
        LabeledPolynomial::new("subset".to_string(), subset_poly.clone(), None, None);
    let labeled_z_i = LabeledPolynomial::new("z_i".to_string(), z_i_poly.clone(), None, None);

    let (subset_commits, _) = KZG::commit(&srs.ck, [&labeled_subset], Some(rng))
        .map_err(|e| ProofError::Commitment(e.to_string()))?;
    let (z_i_commits, _) = KZG::commit(&srs.ck, [&labeled_z_i], Some(rng))
        .map_err(|e| ProofError::Commitment(e.to_string()))?;

    let c_i = subset_commits[0].commitment().comm.0;
    let c_z_i = z_i_commits[0].commitment().comm.0;

    // Step 5: Fiat-Shamir challenge.
    let mut transcript = Transcript::new(b"hera-caulk-plus-v1");
    transcript.append_g1(b"C_table", &table_commitment.commitment);
    transcript.append_g1(b"C_I", &c_i);
    transcript.append_g1(b"C_z_I", &c_z_i);
    for v in &looked_up_values {
        transcript.append_scalar(b"v", v);
    }
    let alpha = transcript.challenge_scalar(b"alpha");

    // Step 6: Evaluate polynomials at alpha.
    let table_eval = table_commitment.poly.evaluate(&alpha);
    let subset_eval = subset_poly.evaluate(&alpha);
    let z_i_eval = z_i_poly.evaluate(&alpha);

    // Step 7: Compute KZG opening proofs at alpha.
    // pi = (poly(x) - poly(alpha)) / (x - alpha)
    let pi_table = compute_opening_proof(srs, &table_commitment.poly, alpha, table_eval, rng)?;
    let pi_subset = compute_opening_proof(srs, &subset_poly, alpha, subset_eval, rng)?;

    Ok(CaulkPlusProof {
        c_i,
        c_z_i,
        pi_table,
        pi_subset,
        table_eval,
        subset_eval,
        z_i_eval,
        looked_up_values,
    })
}

/// Computes a KZG opening proof: commits to (poly(x) - eval) / (x - point).
fn compute_opening_proof(
    srs: &CaulkPlusSrs,
    poly: &DensePolynomial<Fr>,
    point: Fr,
    eval: Fr,
    rng: &mut impl RngCore,
) -> Result<<Bls12_381 as Pairing>::G1Affine, ProofError> {
    // Construct (poly(x) - eval).
    let mut shifted_coeffs = poly.coeffs().to_vec();
    if shifted_coeffs.is_empty() {
        shifted_coeffs.push(-eval);
    } else {
        shifted_coeffs[0] -= eval;
    }
    let numerator = DensePolynomial::from_coefficients_vec(shifted_coeffs);

    // Divide by (x - point).
    let divisor = DensePolynomial::from_coefficients_vec(vec![-point, Fr::ONE]);
    let quotient = poly_div(&numerator, &divisor)?;

    let labeled = LabeledPolynomial::new("quotient".to_string(), quotient, None, None);
    let (commits, _) = KZG::commit(&srs.ck, [&labeled], Some(rng))
        .map_err(|e| ProofError::Commitment(e.to_string()))?;

    Ok(commits[0].commitment().comm.0)
}

/// Polynomial long division. Returns quotient; remainder must be zero
/// for a valid opening proof.
fn poly_div(
    numerator: &DensePolynomial<Fr>,
    divisor: &DensePolynomial<Fr>,
) -> Result<DensePolynomial<Fr>, ProofError> {
    let num_coeffs = numerator.coeffs();
    let div_coeffs = divisor.coeffs();

    if div_coeffs.is_empty() || (div_coeffs.len() == 1 && div_coeffs[0] == Fr::ZERO) {
        return Err(ProofError::ProofGeneration("division by zero polynomial".into()));
    }

    if num_coeffs.len() < div_coeffs.len() {
        return Ok(DensePolynomial::from_coefficients_vec(vec![]));
    }

    let mut remainder = num_coeffs.to_vec();
    let lead_inv = div_coeffs
        .last()
        .unwrap()
        .inverse()
        .ok_or_else(|| ProofError::ProofGeneration("divisor leading coeff is zero".into()))?;

    let mut quotient = vec![Fr::ZERO; remainder.len() - div_coeffs.len() + 1];

    for i in (0..quotient.len()).rev() {
        let coeff = remainder[i + div_coeffs.len() - 1] * lead_inv;
        quotient[i] = coeff;
        for j in 0..div_coeffs.len() {
            remainder[i + j] -= coeff * div_coeffs[j];
        }
    }

    Ok(DensePolynomial::from_coefficients_vec(quotient))
}

/// Lagrange interpolation: given points (x_i, y_i), returns the unique polynomial
/// of degree < n that passes through all points.
fn lagrange_interpolate(xs: &[Fr], ys: &[Fr]) -> DensePolynomial<Fr> {
    assert_eq!(xs.len(), ys.len());
    let n = xs.len();
    if n == 0 {
        return DensePolynomial::from_coefficients_vec(vec![]);
    }

    // result = sum_i y_i * L_i(x), where L_i(x) = prod_{j!=i} (x - x_j)/(x_i - x_j)
    let mut result = vec![Fr::ZERO; n];
    for i in 0..n {
        // Compute the barycentric weight w_i = prod_{j!=i} 1/(x_i - x_j)
        let mut weight = Fr::ONE;
        for j in 0..n {
            if i != j {
                weight *= xs[i] - xs[j];
            }
        }
        weight = weight
            .inverse()
            .expect("duplicate interpolation points");
        weight *= ys[i];

        // Compute L_i(x) * y_i in coefficient form by accumulating
        // weight * prod_{j!=i} (x - x_j)
        let mut basis = vec![Fr::ZERO; n];
        basis[0] = weight;
        for j in 0..n {
            if i == j {
                continue;
            }
            // Multiply current basis by (x - x_j)
            for k in (1..=n - 1).rev() {
                basis[k] = basis[k - 1] - basis[k] * xs[j];
            }
            basis[0] = -basis[0] * xs[j];
        }

        for k in 0..n {
            result[k] += basis[k];
        }
    }

    DensePolynomial::from_coefficients_vec(result)
}
