use aws_sdk_s3::{primitives::ByteStream, types::ServerSideEncryption, Client};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use hera_db::{
    repos::reports::{ReportRepo, StoredArtifactRefs},
    DbPool,
};

use crate::{error::ReporterError, json_manifest::SignedManifest};

/// Captures the immutable object-storage references for one report generation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ArtifactRefs {
    pub json_s3_key: String,
    pub pdf_s3_key: String,
    pub json_sha256: String,
    pub pdf_sha256: String,
}

/// Wraps S3-compatible storage dependencies so uploads and DB updates remain
/// explicit and testable instead of relying on ambient globals.
pub struct ReportStorage {
    client: Client,
    bucket: String,
    db: DbPool,
    kms_key_id: Option<String>,
}

impl ReportStorage {
    pub fn new(
        client: Client,
        bucket: impl Into<String>,
        db: DbPool,
        kms_key_id: Option<String>,
    ) -> Self {
        Self {
            client,
            bucket: bucket.into(),
            db,
            kms_key_id,
        }
    }

    /// Uploads the JSON and PDF artifacts with server-side encryption. Object
    /// keys follow the pattern `reports/{case_id}/{timestamp}.json`. This makes
    /// artifacts addressable, immutable, and auditable by path alone.
    pub async fn store_report(
        &self,
        case_id: Uuid,
        manifest: &SignedManifest,
        pdf: &[u8],
    ) -> Result<ArtifactRefs, ReporterError> {
        let manifest_bytes = manifest.pretty_json_bytes()?;
        let json_sha256 = hex_sha256(&manifest_bytes);
        let pdf_sha256 = hex_sha256(pdf);
        let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
        let json_s3_key = format!("reports/{case_id}/{timestamp}.json");
        let pdf_s3_key = format!("reports/{case_id}/{timestamp}.pdf");

        self.put_object(&json_s3_key, manifest_bytes, "application/json")
            .await?;
        self.put_object(&pdf_s3_key, pdf.to_vec(), "application/pdf")
            .await?;

        let refs = ArtifactRefs {
            json_s3_key,
            pdf_s3_key,
            json_sha256,
            pdf_sha256,
        };

        let report_id = ReportRepo::new(&self.db)
            .upsert_report_artifacts(
                case_id,
                &StoredArtifactRefs {
                    json_s3_key: refs.json_s3_key.clone(),
                    pdf_s3_key: refs.pdf_s3_key.clone(),
                    json_sha256: refs.json_sha256.clone(),
                    pdf_sha256: refs.pdf_sha256.clone(),
                },
            )
            .await
            .map_err(|err| ReporterError::Database(err.to_string()))?;

        ReportRepo::new(&self.db)
            .store_report_signature(report_id, &manifest.signature)
            .await
            .map_err(|err| ReporterError::Database(err.to_string()))?;

        Ok(refs)
    }

    async fn put_object(
        &self,
        key: &str,
        body: Vec<u8>,
        content_type: &str,
    ) -> Result<(), ReporterError> {
        let mut request = self
            .client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .content_type(content_type)
            .body(ByteStream::from(body));

        // Object storage uses SSE-S3 by default, or SSE-KMS when a KMS key is
        // configured, so the stored artifacts are encrypted even at rest.
        request = if let Some(kms_key_id) = &self.kms_key_id {
            request
                .server_side_encryption(ServerSideEncryption::AwsKms)
                .ssekms_key_id(kms_key_id)
        } else {
            request.server_side_encryption(ServerSideEncryption::Aes256)
        };

        request
            .send()
            .await
            .map_err(|err| ReporterError::Storage(err.to_string()))?;
        Ok(())
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}
