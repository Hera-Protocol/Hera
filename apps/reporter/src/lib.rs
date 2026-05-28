#![forbid(unsafe_code)]

pub mod attestation_storage;
pub mod error;
pub mod json_manifest;
pub mod pdf_report;
pub mod s3_client;
pub mod storage;

pub use attestation_storage::AttestationStorage;
pub use error::ReporterError;
pub use json_manifest::{build_manifest, SignedManifest};
pub use pdf_report::build_pdf;
pub use s3_client::build_s3_client;
pub use storage::{ArtifactRefs, ReportStorage};
