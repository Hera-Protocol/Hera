use aws_sdk_s3::{primitives::ByteStream, types::ServerSideEncryption, Client};
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use hera_db::{repos::attestations::AttestationRepo, DbPool};

use crate::error::ReporterError;

#[derive(Debug, Clone)]
pub struct AttestationArtifactRefs {
    pub s3_key: String,
    pub sha256: String,
}

pub struct AttestationStorage {
    client: Client,
    bucket: String,
    db: DbPool,
    kms_key_id: Option<String>,
}

impl AttestationStorage {
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

    pub async fn store_attestation(
        &self,
        case_id: Uuid,
        job_id: Uuid,
        proof_type: &str,
        proof_bytes: &[u8],
        public_inputs_json: &serde_json::Value,
        srs_version: &str,
    ) -> Result<AttestationArtifactRefs, ReporterError> {
        let sha256 = hex_sha256(proof_bytes);
        let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ");
        let s3_key = format!("attestations/{case_id}/{job_id}/{timestamp}.json");

        self.put_object(&s3_key, proof_bytes.to_vec(), "application/json")
            .await?;

        AttestationRepo::new(&self.db)
            .store_attestation(
                case_id,
                job_id,
                proof_type,
                &s3_key,
                &sha256,
                public_inputs_json,
                srs_version,
            )
            .await
            .map_err(|err| ReporterError::Database(err.to_string()))?;

        Ok(AttestationArtifactRefs { s3_key, sha256 })
    }

    pub async fn load_attestation_bytes(&self, key: &str) -> Result<Vec<u8>, ReporterError> {
        let response = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(key)
            .send()
            .await
            .map_err(|err| ReporterError::Storage(err.to_string()))?;

        let bytes = response
            .body
            .collect()
            .await
            .map_err(|err| ReporterError::Storage(err.to_string()))?;

        Ok(bytes.into_bytes().to_vec())
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
