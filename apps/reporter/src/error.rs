use thiserror::Error;

/// Collects artifact-generation failures so canonical JSON, PDF rendering, and
/// storage errors stay explicit across the reporting pipeline.
#[derive(Debug, Error)]
pub enum ReporterError {
    #[error("manifest serialization failed: {0}")]
    ManifestSerialization(String),
    #[error("pdf generation failed: {0}")]
    PdfGeneration(String),
    #[error("storage operation failed: {0}")]
    Storage(String),
    #[error("database update failed: {0}")]
    Database(String),
}
