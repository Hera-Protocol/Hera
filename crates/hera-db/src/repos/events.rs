use chrono::{DateTime, Utc};
use hera_types::{Asset, CanonicalEvent};
use sqlx::{types::Json, FromRow};
use uuid::Uuid;

use crate::{
    build_counterparty, build_memo, build_provenance, chain_to_db, counterparty_visibility_to_db,
    event_type_to_db, network_to_db, parse_chain, parse_event_type, parse_network, DbError, DbPool,
};

/// Owns canonical-event persistence so retries, timelines, and reporting all
/// use one consistent storage path.
pub struct EventRepo<'a> {
    pool: &'a DbPool,
}

impl<'a> EventRepo<'a> {
    pub fn new(pool: &'a DbPool) -> Self {
        Self { pool }
    }

    /// Inserts one canonical event idempotently. Idempotency is critical — scan
    /// jobs can be retried and we must not duplicate events.
    pub async fn insert_canonical_event(&self, event: &CanonicalEvent) -> Result<(), DbError> {
        sqlx::query(
            r#"
            INSERT INTO canonical_events (
                event_id,
                case_id,
                chain,
                network,
                txid,
                block_height,
                timestamp,
                event_type,
                asset_symbol,
                asset_id,
                asset_decimals,
                amount,
                counterparty_visibility,
                counterparty_value,
                memo_present,
                memo_hash,
                evidence_refs,
                provenance_source,
                provenance_pool,
                scan_version,
                notes
            )
            VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                $15, $16, $17, $18, $19, $20, $21
            )
            ON CONFLICT (event_id) DO NOTHING
            "#,
        )
        .bind(event.event_id)
        .bind(event.case_id)
        .bind(chain_to_db(&event.chain))
        .bind(network_to_db(&event.network))
        .bind(&event.txid)
        .bind(
            i64::try_from(event.block_height)
                .map_err(|_| DbError::InvalidData("block height overflow".into()))?,
        )
        .bind(event.timestamp)
        .bind(event_type_to_db(&event.event_type))
        .bind(&event.asset.symbol)
        .bind(&event.asset.asset_id)
        .bind(i16::from(event.asset.decimals))
        .bind(&event.amount)
        .bind(counterparty_visibility_to_db(
            &event.counterparty.visibility,
        ))
        .bind(&event.counterparty.value)
        .bind(event.memo.present)
        .bind(&event.memo.hash)
        .bind(Json(&event.evidence_refs))
        .bind(&event.provenance.source)
        .bind(&event.provenance.pool)
        .bind(&event.provenance.scan_version)
        .bind(Json(&event.notes))
        .execute(self.pool)
        .await
        .map_err(DbError::Query)?;

        Ok(())
    }

    /// Loads all canonical events for a case so report generation can operate on
    /// the normalized record set rather than chain-specific raw data.
    pub async fn get_events_for_case(&self, case_id: Uuid) -> Result<Vec<CanonicalEvent>, DbError> {
        self.get_event_timeline(case_id, None, None).await
    }

    /// Returns a case timeline with optional temporal filters so APIs and reports
    /// can page through a bounded evidence window deterministically.
    pub async fn get_event_timeline(
        &self,
        case_id: Uuid,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> Result<Vec<CanonicalEvent>, DbError> {
        let rows = sqlx::query_as::<_, EventRow>(
            r#"
            SELECT
                event_id,
                case_id,
                chain,
                network,
                txid,
                block_height,
                timestamp,
                event_type,
                asset_symbol,
                asset_id,
                asset_decimals,
                amount,
                counterparty_visibility,
                counterparty_value,
                memo_present,
                memo_hash,
                evidence_refs,
                provenance_source,
                provenance_pool,
                scan_version,
                notes
            FROM canonical_events
            WHERE case_id = $1
              AND ($2::timestamptz IS NULL OR timestamp >= $2)
              AND ($3::timestamptz IS NULL OR timestamp <= $3)
            ORDER BY timestamp ASC, block_height ASC, event_id ASC
            "#,
        )
        .bind(case_id)
        .bind(from)
        .bind(to)
        .fetch_all(self.pool)
        .await
        .map_err(DbError::Query)?;

        rows.into_iter().map(EventRow::try_into_event).collect()
    }
}

#[derive(Debug, FromRow)]
struct EventRow {
    event_id: Uuid,
    case_id: Uuid,
    chain: String,
    network: String,
    txid: String,
    block_height: i64,
    timestamp: DateTime<Utc>,
    event_type: String,
    asset_symbol: String,
    asset_id: String,
    asset_decimals: i16,
    amount: String,
    counterparty_visibility: String,
    counterparty_value: Option<String>,
    memo_present: bool,
    memo_hash: Option<String>,
    evidence_refs: Json<Vec<String>>,
    provenance_source: String,
    provenance_pool: Option<String>,
    scan_version: String,
    notes: Json<Vec<String>>,
}

impl EventRow {
    fn try_into_event(self) -> Result<CanonicalEvent, DbError> {
        let block_height = u64::try_from(self.block_height).map_err(|_| {
            DbError::InvalidData("negative block height in canonical_events".into())
        })?;
        let asset_decimals = u8::try_from(self.asset_decimals)
            .map_err(|_| DbError::InvalidData("asset decimals out of range".into()))?;

        Ok(CanonicalEvent {
            event_id: self.event_id,
            case_id: self.case_id,
            chain: parse_chain(&self.chain)?,
            network: parse_network(&self.network)?,
            event_type: parse_event_type(&self.event_type)?,
            txid: self.txid,
            block_height,
            timestamp: self.timestamp,
            asset: Asset {
                symbol: self.asset_symbol,
                asset_id: self.asset_id,
                decimals: asset_decimals,
            },
            amount: self.amount,
            counterparty: build_counterparty(
                &self.counterparty_visibility,
                self.counterparty_value,
            )?,
            memo: build_memo(self.memo_present, self.memo_hash),
            evidence_refs: self.evidence_refs.0,
            provenance: build_provenance(
                self.provenance_source,
                self.provenance_pool,
                self.scan_version,
            ),
            notes: self.notes.0,
        })
    }
}
