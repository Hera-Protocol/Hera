use aws_config::{BehaviorVersion, Region};

/// Builds the shared S3 client used by Hera services so endpoint handling stays
/// consistent across the API and worker runtimes.
pub async fn build_s3_client(region: &str, endpoint_url: Option<&str>) -> aws_sdk_s3::Client {
    let shared_config = aws_config::defaults(BehaviorVersion::latest())
        .region(Region::new(region.to_string()))
        .load()
        .await;

    let mut builder = aws_sdk_s3::config::Builder::from(&shared_config);
    if let Some(endpoint_url) = endpoint_url {
        // S3-compatible emulators such as LocalStack expect path-style access.
        // We only force it when overriding the endpoint so AWS production stays
        // on the SDK defaults.
        builder = builder.endpoint_url(endpoint_url).force_path_style(true);
    }

    aws_sdk_s3::Client::from_conf(builder.build())
}
