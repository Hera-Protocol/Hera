use hera_namada_adapter::MaspIndexerClient;

fn namada_indexer_url() -> Option<String> {
    std::env::var("NAMADA_INDEXER_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .filter(|value| !value.contains("your-namada-indexer"))
}

#[tokio::test]
#[ignore]
async fn indexer_reports_latest_block_cursor() {
    let Some(indexer_url) = namada_indexer_url() else {
        eprintln!("skipping Namada live smoke test because NAMADA_INDEXER_URL is not configured");
        return;
    };

    let cursor = MaspIndexerClient { indexer_url }
        .latest_indexed_block()
        .await
        .unwrap_or_else(|err| panic!("failed to read live Namada indexer cursor: {err}"));

    assert!(cursor.height > 0);
}
