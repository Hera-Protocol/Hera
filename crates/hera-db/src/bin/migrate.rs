use std::env;

/// Runs the shared migration set without booting an API or worker process so
/// local setup and CI can advance schema state explicitly when needed.
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let database_url =
        env::var("DATABASE_URL").map_err(|_| anyhow::anyhow!("missing DATABASE_URL"))?;
    let _pool = hera_db::connect(&database_url).await?;
    Ok(())
}
