use hera_namada_adapter::MaspIndexerClient;

fn namada_indexer_url() -> (String, bool) {
    match std::env::var("NAMADA_INDEXER_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .filter(|value| !value.contains("your-namada-indexer"))
    {
        Some(value) => (value, true),
        None => ("https://masp.namada.net".to_string(), false),
    }
}

#[tokio::test]
#[ignore]
async fn indexer_reports_latest_block_cursor() {
    let (indexer_url, explicit_endpoint) = namada_indexer_url();
    let cursor = match (MaspIndexerClient { indexer_url }).latest_indexed_block().await {
        Ok(value) => value,
        Err(err) if !explicit_endpoint => {
            eprintln!(
                "skipping Namada live smoke test because the default MASP indexer was unreachable: {err}"
            );
            return;
        }
        Err(err) => panic!("failed to read live Namada indexer cursor: {err}"),
    };

    assert!(cursor.height > 0);
}

#[tokio::test]
#[ignore]
async fn indexer_serves_recent_masp_tx_window() {
    let (indexer_url, explicit_endpoint) = namada_indexer_url();
    let client = MaspIndexerClient {
        indexer_url,
    };
    let latest = match client.latest_indexed_block().await {
        Ok(value) => value,
        Err(err) if !explicit_endpoint => {
            eprintln!(
                "skipping Namada live smoke test because the default MASP indexer was unreachable: {err}"
            );
            return;
        }
        Err(err) => panic!("failed to read live Namada indexer cursor: {err}"),
    };
    let from_block = latest.height.saturating_sub(10);

    let context = match client
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
    {
        Ok(value) => value,
        Err(err) if !explicit_endpoint => {
            eprintln!(
                "skipping Namada live smoke test because the default MASP indexer was unreachable: {err}"
            );
            return;
        }
        Err(err) => panic!("failed to fetch live Namada tx window: {err}"),
    };

    assert!(context.last_synced_block() >= from_block);
    let public_state = context
        .public_state()
        .unwrap_or_else(|| panic!("expected public MASP state in live context"));
    assert!(public_state.has_auxiliary_state());
}
