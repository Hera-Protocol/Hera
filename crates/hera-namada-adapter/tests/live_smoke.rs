use hera_namada_adapter::MaspIndexerClient;

fn namada_indexer_url() -> String {
    std::env::var("NAMADA_INDEXER_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .filter(|value| !value.contains("your-namada-indexer"))
        .unwrap_or_else(|| "https://masp.namada.net".to_string())
}

#[tokio::test]
#[ignore]
async fn indexer_reports_latest_block_cursor() {
    let cursor = MaspIndexerClient {
        indexer_url: namada_indexer_url(),
    }
    .latest_indexed_block()
    .await
    .unwrap_or_else(|err| panic!("failed to read live Namada indexer cursor: {err}"));

    assert!(cursor.height > 0);
}

#[tokio::test]
#[ignore]
async fn indexer_serves_recent_masp_tx_window() {
    let client = MaspIndexerClient {
        indexer_url: namada_indexer_url(),
    };
    let latest = client
        .latest_indexed_block()
        .await
        .unwrap_or_else(|err| panic!("failed to read live Namada indexer cursor: {err}"));
    let from_block = latest.height.saturating_sub(10);

    let context = client
        .fetch_shielded_context(
            &hera_namada_adapter::ValidatedNamadaKey {
                raw_key: "zvknam1smoketest".into(),
                chain_id: "namada.5f5de2dd1b88cba30586420".into(),
                birthday_height: Some(from_block),
            },
            from_block,
            |_| {},
        )
        .await
        .unwrap_or_else(|err| panic!("failed to fetch live Namada tx window: {err}"));

    assert!(context.last_synced_block() >= from_block);
}
