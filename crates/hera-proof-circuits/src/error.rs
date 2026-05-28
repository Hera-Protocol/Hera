use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProofError {
    #[error("srs generation failed: {0}")]
    SrsGeneration(String),
    #[error("commitment failed: {0}")]
    Commitment(String),
    #[error("proof generation failed: {0}")]
    ProofGeneration(String),
    #[error("verification failed: {0}")]
    Verification(String),
    #[error("serialization failed: {0}")]
    Serialization(String),
    #[error("witness error: {0}")]
    Witness(#[from] hera_proof_witness::WitnessError),
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
}
