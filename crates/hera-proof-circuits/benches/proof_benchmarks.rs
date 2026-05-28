use criterion::{criterion_group, criterion_main, Criterion};

use ark_bls12_381::Fr;
use ark_ff::PrimeField;
use hera_proof_circuits::{
    caulk::{
        commitment::commit_table, prover::prove as caulk_prove, srs::CaulkPlusSrs,
        verifier::verify as caulk_verify,
    },
    circuits::{
        risk_score::{prove_risk_below, verify_risk_below, RiskScoreStatement},
        threshold::{prove_threshold, verify_threshold, ThresholdStatement},
    },
};
use hera_proof_witness::WitnessRecord;

fn make_records(count: usize) -> Vec<WitnessRecord> {
    (0..count)
        .map(|i| WitnessRecord {
            amount: Fr::from((i as u64 + 1) * 100),
            event_type: Fr::from(1u64), // All Receive
            txid_hash: Fr::from(i as u64 + 1000),
            block_height: Fr::from(100u64),
            timestamp: Fr::from(1000000u64),
            risk_score: Fr::from(10u64),
        })
        .collect()
}

fn bench_caulk_prove_verify(c: &mut Criterion) {
    let mut rng = ark_std::test_rng();

    for size in [16, 64] {
        let srs = CaulkPlusSrs::generate(size * 4, &mut rng).unwrap();
        let table: Vec<Fr> = (0..size).map(|i| Fr::from(i as u64)).collect();
        let tc = commit_table(&srs, &table, &mut rng).unwrap();
        let indices: Vec<usize> = (0..4).collect();

        c.bench_function(&format!("caulk_prove_N{size}_m4"), |b| {
            b.iter(|| caulk_prove(&srs, &tc, &indices, &mut ark_std::test_rng()).unwrap())
        });

        let proof = caulk_prove(&srs, &tc, &indices, &mut rng).unwrap();
        c.bench_function(&format!("caulk_verify_N{size}_m4"), |b| {
            b.iter(|| caulk_verify(&srs, &tc, &proof).unwrap())
        });
    }
}

fn bench_threshold_circuit(c: &mut Criterion) {
    let mut rng = ark_std::test_rng();

    for count in [8, 32] {
        let srs = CaulkPlusSrs::generate(count * 4, &mut rng).unwrap();
        let records = make_records(count);
        let statement = ThresholdStatement {
            threshold_raw: 100,
            event_count: count,
        };

        c.bench_function(&format!("threshold_prove_N{count}"), |b| {
            b.iter(|| {
                prove_threshold(&srs, &records, &statement, &mut ark_std::test_rng()).unwrap()
            })
        });
    }
}

fn bench_risk_circuit(c: &mut Criterion) {
    let mut rng = ark_std::test_rng();

    for count in [8, 32] {
        let srs = CaulkPlusSrs::generate(count * 4, &mut rng).unwrap();
        let records = make_records(count);
        let statement = RiskScoreStatement {
            max_risk: 50,
            event_count: count,
        };

        c.bench_function(&format!("risk_prove_N{count}"), |b| {
            b.iter(|| {
                prove_risk_below(&srs, &records, &statement, &mut ark_std::test_rng()).unwrap()
            })
        });
    }
}

criterion_group!(
    benches,
    bench_caulk_prove_verify,
    bench_threshold_circuit,
    bench_risk_circuit
);
criterion_main!(benches);
