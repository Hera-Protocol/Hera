use std::io::{Read as _, Write as _};

use anyhow::Context as _;
use hera_namada_adapter::{ExternalDecodeRequest, ExternalDecodeResponse, MaspNote};

/// Reads the external decoder request from stdin and returns a JSON response on
/// stdout. This binary is intentionally isolated from the main Hera workspace
/// so a future official Namada SDK integration can live here without forcing
/// those heavier dependencies into every Stage 1 crate.
#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut buffer = Vec::new();
    stdin
        .read_to_end(&mut buffer)
        .context("failed to read decoder request from stdin")?;

    let request = serde_json::from_slice::<ExternalDecodeRequest>(&buffer)
        .context("failed to parse decoder request json")?;
    let response = decode_request(request).await?;

    serde_json::to_writer(&mut stdout, &response).context("failed to write decoder response")?;
    stdout.flush().context("failed to flush decoder response")?;
    Ok(())
}

async fn decode_request(request: ExternalDecodeRequest) -> anyhow::Result<ExternalDecodeResponse> {
    decode_request_with_policy(
        request,
        std::env::var("HERA_NAMADA_DECODER_ALLOW_STUB")
            .ok()
            .as_deref()
            == Some("1"),
    )
    .await
}

async fn decode_request_with_policy(
    request: ExternalDecodeRequest,
    allow_stub: bool,
) -> anyhow::Result<ExternalDecodeResponse> {
    if request.public_state.tx_count() == 0 {
        return Ok(ExternalDecodeResponse { notes: Vec::new() });
    }

    if allow_stub {
        return Ok(ExternalDecodeResponse {
            notes: build_stub_notes(&request),
        });
    }

    anyhow::bail!(
        "stub decoder received {} indexed transactions; replace this binary with an official Namada SDK-backed decoder before using it for production scans",
        request.public_state.tx_count()
    );
}

fn build_stub_notes(request: &ExternalDecodeRequest) -> Vec<MaspNote> {
    request
        .public_state
        .block_heights()
        .into_iter()
        .enumerate()
        .map(|(index, block_height)| MaspNote {
            txid: format!("stub-tx-{index}"),
            block_height,
            timestamp: chrono::Utc::now(),
            asset: hera_types::Asset {
                symbol: "NAM".into(),
                asset_id: "nam".into(),
                decimals: 6,
            },
            amount_raw: 0,
            note_commitment: format!("stub-commitment-{index}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use hera_namada_adapter::{ExternalDecodeRequest, ExternalDecodeResponse};

    use super::{build_stub_notes, decode_request, decode_request_with_policy};

    fn sample_request() -> ExternalDecodeRequest {
        serde_json::from_value(serde_json::json!({
            "key": {
                "raw_key": "zvknam1qqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqqq",
                "chain_id": "namada.5f5de2dd1b88cba30586420",
                "birthday_height": 10
            },
            "public_state": {
                "txs": [{
                    "block_height": 42,
                    "block_index": 0,
                    "batch": [{
                        "masp_tx_index": 0,
                        "is_masp_fee_payment": false,
                        "bytes": [1, 2, 3]
                    }]
                }],
                "note_index": [{ "note_pos": 1 }],
                "witness_map": { "witness": ["abc"] },
                "commitment_tree": [{ "root": "root-1" }]
            }
        }))
        .unwrap_or_else(|err| panic!("failed to build sample request: {err}"))
    }

    #[tokio::test]
    async fn rejects_non_empty_requests_without_stub_flag() {
        let result = decode_request(sample_request()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn returns_stub_notes_when_enabled() {
        let response = decode_request_with_policy(sample_request(), true)
            .await
            .unwrap_or_else(|err| panic!("unexpected stub decoder failure: {err}"));

        let ExternalDecodeResponse { notes } = response;
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].block_height, 42);
        assert_eq!(notes[0].asset.asset_id, "nam");
    }

    #[test]
    fn stub_notes_follow_block_heights() {
        let request = sample_request();
        let notes = build_stub_notes(&request);

        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].block_height, 42);
        assert_eq!(notes[0].amount_raw, 0);
    }
}
