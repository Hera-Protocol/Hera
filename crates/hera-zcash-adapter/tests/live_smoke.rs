use hera_types::Network;
use hera_zcash_adapter::ZcashScanner;

fn lightwalletd_urls() -> (Vec<String>, bool) {
    let explicit_primary = std::env::var("LIGHTWALLETD_URL")
        .ok()
        .filter(|value| !value.trim().is_empty());
    let explicit_fallbacks = std::env::var("LIGHTWALLETD_FALLBACK_URLS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let explicit = explicit_primary.is_some() || !explicit_fallbacks.is_empty();
    let mut urls = Vec::new();
    if let Some(primary) = explicit_primary {
        urls.push(primary);
    } else {
        urls.push("https://mainnet.lightwalletd.com:9067".to_string());
    }
    urls.extend(explicit_fallbacks);
    (urls, explicit)
}

#[tokio::test]
#[ignore]
async fn lightwalletd_reports_latest_block_height() {
    let (lightwalletd_urls, explicit_endpoint) = lightwalletd_urls();
    let scanner = ZcashScanner {
        lightwalletd_urls,
        network: Network::Mainnet,
    };

    let endpoint = match scanner.resolve_endpoint().await {
        Ok(value) => value,
        Err(err) if !explicit_endpoint => {
            eprintln!("skipping Zcash live smoke test because no default lightwalletd endpoint was reachable: {err}");
            return;
        }
        Err(err) => panic!("failed to resolve a live lightwalletd endpoint: {err}"),
    };

    assert!(endpoint.tip_height > 0);
    assert!(!endpoint.url.is_empty());
}
