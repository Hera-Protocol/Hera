use std::process::Stdio;

use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt as _, process::Command};

use crate::{
    detect_owned_notes,
    error::NamadaAdapterError,
    masp_sync::{PublicIndexerState, ShieldedContext},
    types::MaspNote,
    viewing_key::ValidatedNamadaKey,
};

/// Defines how Hera can delegate Namada MASP decoding to an isolated external
/// binary when the in-process adapter intentionally avoids pulling in the
/// heavier official SDK stack. This preserves the privacy boundary while
/// keeping the Stage 1 workspace license surface small.
#[derive(Debug, Clone)]
pub struct ExternalMaspDecoder {
    pub command: String,
    pub args: Vec<String>,
}

/// Carries the raw public-indexer snapshot plus the viewing-key context into an
/// isolated decoder. The decoder receives everything over stdin so we do not
/// leak viewing keys via command-line arguments or process listings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExternalDecodeRequest {
    pub key: ValidatedNamadaKey,
    pub public_state: PublicIndexerState,
}

/// Defines the response contract from an external MASP decoder. The decoder is
/// responsible only for ownership detection; Hera keeps normalization and
/// persistence in-process.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ExternalDecodeResponse {
    pub notes: Vec<MaspNote>,
}

/// Uses the configured external decoder when public MASP state is present and
/// falls back to the legacy in-process path for pre-decoded indexer entries.
pub async fn decode_owned_notes_with_external(
    context: &ShieldedContext,
    key: &ValidatedNamadaKey,
    decoder: &ExternalMaspDecoder,
) -> Result<Vec<MaspNote>, NamadaAdapterError> {
    let Some(public_state) = context.public_state() else {
        return detect_owned_notes(context, key);
    };

    if public_state.tx_count() == 0 {
        return Ok(Vec::new());
    }

    let request = ExternalDecodeRequest {
        key: key.clone(),
        public_state: public_state.clone(),
    };
    let payload = serde_json::to_vec(&request)
        .map_err(|err| NamadaAdapterError::ExternalDecoderProtocol(err.to_string()))?;
    let mut child = Command::new(&decoder.command)
        .args(&decoder.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| NamadaAdapterError::ExternalDecoderFailed(err.to_string()))?;

    let mut stdin = child.stdin.take().ok_or_else(|| {
        NamadaAdapterError::ExternalDecoderFailed(
            "failed to open stdin for external decoder".to_string(),
        )
    })?;
    stdin
        .write_all(&payload)
        .await
        .map_err(|err| NamadaAdapterError::ExternalDecoderFailed(err.to_string()))?;
    drop(stdin);

    let output = child
        .wait_with_output()
        .await
        .map_err(|err| NamadaAdapterError::ExternalDecoderFailed(err.to_string()))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(NamadaAdapterError::ExternalDecoderFailed(format!(
            "decoder exited with {}: {}",
            output.status, stderr
        )));
    }

    let response = serde_json::from_slice::<ExternalDecodeResponse>(&output.stdout)
        .map_err(|err| NamadaAdapterError::ExternalDecoderProtocol(err.to_string()))?;
    Ok(response.notes)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    use super::{decode_owned_notes_with_external, ExternalMaspDecoder};
    use crate::{
        masp_sync::{IndexedMaspBatchItem, IndexedMaspTx, PublicIndexerState, ShieldedContext},
        viewing_key::ValidatedNamadaKey,
    };

    fn ts(year: i32, month: u32, day: u32, hour: u32, min: u32, sec: u32) -> chrono::DateTime<Utc> {
        match Utc.with_ymd_and_hms(year, month, day, hour, min, sec) {
            chrono::LocalResult::Single(value) => value,
            other => panic!("unexpected timestamp result: {other:?}"),
        }
    }

    fn sample_context() -> ShieldedContext {
        ShieldedContext::new_public(
            PublicIndexerState::new(
                vec![IndexedMaspTx {
                    block_height: 22,
                    block_index: 0,
                    batch: vec![IndexedMaspBatchItem {
                        masp_tx_index: 0,
                        is_masp_fee_payment: false,
                        bytes: vec![1, 2, 3],
                    }],
                }],
                json!([{ "note_pos": 7 }]),
                json!({ "witness": ["abc"] }),
                json!([{ "root": "root-1" }]),
            ),
            22,
            22,
        )
    }

    fn sample_key() -> ValidatedNamadaKey {
        ValidatedNamadaKey {
            raw_key: "zvknam1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq".into(),
            chain_id: "namada.5f5de2dd1b88cba30586420".into(),
            birthday_height: Some(10),
        }
    }

    #[tokio::test]
    async fn external_decoder_returns_notes() {
        let decoder = ExternalMaspDecoder {
            command: "sh".into(),
            args: vec![
                "-c".into(),
                "cat >/dev/null; printf '%s' '{\"notes\":[{\"txid\":\"tx-1\",\"block_height\":22,\"timestamp\":\"2025-04-05T06:07:08Z\",\"asset\":{\"symbol\":\"NAM\",\"asset_id\":\"nam\",\"decimals\":6},\"amount_raw\":12345,\"note_commitment\":\"commit-1\"}]}'".into(),
            ],
        };

        let notes = decode_owned_notes_with_external(&sample_context(), &sample_key(), &decoder)
            .await
            .unwrap_or_else(|err| panic!("unexpected external decode failure: {err}"));

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].txid, "tx-1");
        assert_eq!(notes[0].amount_raw, 12_345);
        assert_eq!(notes[0].timestamp, ts(2025, 4, 5, 6, 7, 8));
    }

    #[tokio::test]
    async fn external_decoder_rejects_invalid_json() {
        let decoder = ExternalMaspDecoder {
            command: "sh".into(),
            args: vec!["-c".into(), "cat >/dev/null; printf 'not-json'".into()],
        };

        let result =
            decode_owned_notes_with_external(&sample_context(), &sample_key(), &decoder).await;

        assert!(result.is_err());
        let err = result.err().expect("expected external decoder failure");
        assert!(err.to_string().contains("invalid output"));
    }
}
