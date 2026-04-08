use hera_types::Network;
use hera_zcash_adapter::ZcashScanner;

fn lightwalletd_url() -> (String, bool) {
    match std::env::var("LIGHTWALLETD_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        Some(value) => (value, true),
        None => ("https://mainnet.lightwalletd.com:9067".to_string(), false),
    }
}

#[tokio::test]
#[ignore]
async fn lightwalletd_reports_latest_block_height() {
    let (lightwalletd_url, explicit_endpoint) = lightwalletd_url();
    let scanner = ZcashScanner {
        lightwalletd_url,
        network: Network::Mainnet,
    };

    let height = match scanner.latest_block_height().await {
        Ok(value) => value,
        Err(err) if !explicit_endpoint => {
            eprintln!("skipping Zcash live smoke test because the default lightwalletd endpoint was unreachable: {err}");
            return;
        }
        Err(err) => panic!("failed to read live lightwalletd tip: {err}"),
    };

    assert!(height > 0);
}
