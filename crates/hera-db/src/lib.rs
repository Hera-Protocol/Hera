#![forbid(unsafe_code)]

pub mod error;
pub mod repos;

use hera_types::{
    ChainId, Counterparty, CounterpartyVisibility, EventMemo, EventProvenance, EventType, Network,
    ScanJobStatus,
};
use sqlx::{postgres::PgPoolOptions, PgPool};

pub use error::DbError;

/// Shared pool type for Hera's Postgres access. Other crates depend on this
/// alias instead of importing `sqlx` directly.
pub type DbPool = PgPool;

/// Connects to Postgres and applies any pending migrations before returning a
/// ready-to-use pool. Migrations run at startup. This is intentional — it keeps
/// deployments simple and ensures the schema is always current.
pub async fn connect(database_url: &str) -> Result<DbPool, DbError> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
        .map_err(DbError::Connect)?;

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(DbError::Migration)?;

    Ok(pool)
}

pub(crate) fn chain_to_db(chain: &ChainId) -> &'static str {
    match chain {
        ChainId::Zcash => "ZCASH",
        ChainId::Namada => "NAMADA",
    }
}

pub(crate) fn network_to_db(network: &Network) -> &'static str {
    match network {
        Network::Mainnet => "MAINNET",
        Network::Testnet => "TESTNET",
        Network::Regtest => "REGTEST",
    }
}

pub(crate) fn event_type_to_db(event_type: &EventType) -> &'static str {
    match event_type {
        EventType::Shield => "SHIELD",
        EventType::Receive => "RECEIVE",
        EventType::Send => "SEND",
        EventType::Unshield => "UNSHIELD",
        EventType::Fee => "FEE",
    }
}

pub(crate) fn counterparty_visibility_to_db(visibility: &CounterpartyVisibility) -> &'static str {
    match visibility {
        CounterpartyVisibility::Known => "KNOWN",
        CounterpartyVisibility::Unknown => "UNKNOWN",
        CounterpartyVisibility::Partial => "PARTIAL",
    }
}

pub(crate) fn scan_job_status_to_db(status: &ScanJobStatus) -> (&'static str, Option<&str>) {
    match status {
        ScanJobStatus::Created => ("CREATED", None),
        ScanJobStatus::KeyValidated => ("KEY_VALIDATED", None),
        ScanJobStatus::ChainSyncing => ("CHAIN_SYNCING", None),
        ScanJobStatus::DetectingNotes => ("DETECTING_NOTES", None),
        ScanJobStatus::ClassifyingFlows => ("CLASSIFYING_FLOWS", None),
        ScanJobStatus::BuildingReport => ("BUILDING_REPORT", None),
        ScanJobStatus::Signed => ("SIGNED", None),
        ScanJobStatus::Failed(reason) => ("FAILED", Some(reason.as_str())),
    }
}

pub(crate) fn parse_chain(value: &str) -> Result<ChainId, DbError> {
    match value {
        "ZCASH" => Ok(ChainId::Zcash),
        "NAMADA" => Ok(ChainId::Namada),
        other => Err(DbError::InvalidData(format!(
            "unsupported chain value: {other}"
        ))),
    }
}

pub(crate) fn parse_network(value: &str) -> Result<Network, DbError> {
    match value {
        "MAINNET" => Ok(Network::Mainnet),
        "TESTNET" => Ok(Network::Testnet),
        "REGTEST" => Ok(Network::Regtest),
        other => Err(DbError::InvalidData(format!(
            "unsupported network value: {other}"
        ))),
    }
}

pub(crate) fn parse_event_type(value: &str) -> Result<EventType, DbError> {
    match value {
        "SHIELD" => Ok(EventType::Shield),
        "RECEIVE" => Ok(EventType::Receive),
        "SEND" => Ok(EventType::Send),
        "UNSHIELD" => Ok(EventType::Unshield),
        "FEE" => Ok(EventType::Fee),
        other => Err(DbError::InvalidData(format!(
            "unsupported event type: {other}"
        ))),
    }
}

pub(crate) fn parse_scan_job_status(
    value: &str,
    failure_reason: Option<String>,
) -> Result<ScanJobStatus, DbError> {
    match value {
        "CREATED" => Ok(ScanJobStatus::Created),
        "KEY_VALIDATED" => Ok(ScanJobStatus::KeyValidated),
        "CHAIN_SYNCING" => Ok(ScanJobStatus::ChainSyncing),
        "DETECTING_NOTES" => Ok(ScanJobStatus::DetectingNotes),
        "CLASSIFYING_FLOWS" => Ok(ScanJobStatus::ClassifyingFlows),
        "BUILDING_REPORT" => Ok(ScanJobStatus::BuildingReport),
        "SIGNED" => Ok(ScanJobStatus::Signed),
        "FAILED" => Ok(ScanJobStatus::Failed(
            failure_reason.unwrap_or_else(|| "scan job failed".to_string()),
        )),
        other => Err(DbError::InvalidData(format!(
            "unsupported scan job status value: {other}"
        ))),
    }
}

pub(crate) fn parse_counterparty_visibility(
    value: &str,
) -> Result<CounterpartyVisibility, DbError> {
    match value {
        "KNOWN" => Ok(CounterpartyVisibility::Known),
        "UNKNOWN" => Ok(CounterpartyVisibility::Unknown),
        "PARTIAL" => Ok(CounterpartyVisibility::Partial),
        other => Err(DbError::InvalidData(format!(
            "unsupported counterparty visibility: {other}"
        ))),
    }
}

pub(crate) fn build_counterparty(
    visibility: &str,
    value: Option<String>,
) -> Result<Counterparty, DbError> {
    Ok(Counterparty {
        visibility: parse_counterparty_visibility(visibility)?,
        value,
    })
}

pub(crate) fn build_memo(present: bool, hash: Option<String>) -> EventMemo {
    EventMemo { present, hash }
}

pub(crate) fn build_provenance(
    source: String,
    pool: Option<String>,
    scan_version: String,
) -> EventProvenance {
    EventProvenance {
        source,
        pool,
        scan_version,
    }
}
