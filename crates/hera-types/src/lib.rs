#![forbid(unsafe_code)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Identifies which chain produced a compliance event so everything above the
/// adapter layer can stay chain-agnostic while still preserving provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChainId {
    Zcash,
    Namada,
}

/// Distinguishes the network context because the same key or transaction shape
/// can mean very different things across production, test, and local chains.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Network {
    Mainnet,
    Testnet,
    Regtest,
}

/// Buckets chain-specific transfer mechanics into stable compliance categories
/// so downstream policy, reporting, and audit systems do not need per-chain logic.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EventType {
    Shield,
    Receive,
    Send,
    Unshield,
    Fee,
}

/// Carries asset identity without baking chain asset catalogs into code because
/// Namada and IBC assets are dynamic and cannot be exhaustively enumerated ahead of time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct Asset {
    /// Gives humans a stable display label for reports and investigator review.
    pub symbol: String,
    /// Uses a string because Namada is multi-asset and IBC assets are dynamic,
    /// so compile-time enums would become a migration bottleneck and silently age out.
    pub asset_id: String,
    /// Preserves the chain-declared precision so exact decimal rendering can happen
    /// later without re-querying chain metadata or guessing formatting rules.
    pub decimals: u8,
}

/// Captures how much of the other side of a transfer we can honestly observe so
/// compliance artifacts can distinguish certainty from partial visibility.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CounterpartyVisibility {
    Known,
    Unknown,
    Partial,
}

/// Represents counterparty claims conservatively because shielded systems often
/// reveal less than compliance consumers wish they could see.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct Counterparty {
    /// Encodes the observation boundary so reports can say what is known, unknown,
    /// or only partially visible without overstating certainty.
    pub visibility: CounterpartyVisibility,
    /// Holds the observed counterparty identifier when one exists; we never lie
    /// about what we can see.
    pub value: Option<String>,
}

/// Tracks memo presence separately from memo contents because a memo can matter
/// for compliance even when it cannot be decrypted or safely retained.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct EventMemo {
    /// Indicates whether any memo was attached so investigators can reason about
    /// hidden context even when the bytes are not readable.
    pub present: bool,
    /// Stores a digest when available so memo evidence can be correlated without
    /// embedding sensitive payloads into the canonical event.
    pub hash: Option<String>,
}

/// Preserves scan lineage so every canonical event can be traced back to the
/// adapter, pool, and engine version that produced it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct EventProvenance {
    /// Names the source domain so auditors can tell whether evidence came from
    /// compact blocks, an indexer, or another chain-specific source.
    pub source: String,
    /// Records the chain pool or subsystem so shielded origins remain explicit
    /// instead of being flattened away during normalization.
    pub pool: Option<String>,
    /// Pins the exact scan engine version to make artifacts reproducible across
    /// reruns, upgrades, and future dispute resolution.
    pub scan_version: String,
}

/// Defines the witness-ready compliance record that every adapter normalizes into,
/// giving downstream storage, reporting, and policy systems one stable schema.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CanonicalEvent {
    /// Provides a stable unique identifier so inserts can be made idempotent and
    /// the same chain event is never counted twice across retries.
    pub event_id: Uuid,
    /// Binds the event to the compliance case that authorized the scan and owns
    /// the resulting evidence trail.
    pub case_id: Uuid,
    /// Records which chain produced the event so policy engines can apply the
    /// correct regulatory interpretation without re-inspecting raw evidence.
    pub chain: ChainId,
    /// Records the network so testnet and local evidence cannot be mistaken for
    /// production activity in reports or audits.
    pub network: Network,
    /// Stores the normalized compliance category used for reporting, filtering,
    /// and future risk evaluation.
    pub event_type: EventType,
    /// Retains the transaction identifier so investigators can anchor the event
    /// back to chain-visible evidence or third-party indexers.
    pub txid: String,
    /// Captures ledger ordering so timelines can be reconstructed and scan
    /// checkpoints can be correlated with the event lifecycle.
    pub block_height: u64,
    /// Provides the authoritative event time used in exports, timelines, and any
    /// later policy rules that depend on temporal ordering.
    pub timestamp: DateTime<Utc>,
    /// Preserves asset identity and precision because the same compliance case can
    /// include many assets that must never be conflated.
    pub asset: Asset,
    /// Uses a decimal string to preserve exact on-chain value representation and
    /// avoid precision loss in reports, signatures, and downstream consumers.
    pub amount: String,
    /// Encodes what we know about the other side of the transfer so the record is
    /// explicit about visibility limits instead of relying on guesswork.
    pub counterparty: Counterparty,
    /// Tracks memo evidence without turning memo contents into mandatory plaintext
    /// storage, which would create unnecessary retention risk.
    pub memo: EventMemo,
    /// Holds deterministic evidence references so a verifier can reproduce or
    /// independently inspect the raw sources behind this canonical record.
    pub evidence_refs: Vec<String>,
    /// Captures chain-specific lineage needed to explain how this event was
    /// derived while keeping the top-level schema chain-agnostic.
    pub provenance: EventProvenance,
    /// Stores human-readable compliance annotations, such as when a memo existed
    /// but could not be decrypted, without mutating core normalized fields.
    pub notes: Vec<String>,
}

/// Describes the lifecycle stage of a scan job so operators, APIs, and audit logs
/// can all speak the same state machine language.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ScanJobStatus {
    Created,
    KeyValidated,
    ChainSyncing,
    DetectingNotes,
    ClassifyingFlows,
    BuildingReport,
    Signed,
    Failed(String),
}

/// Represents the durable control-plane record for a scan so orchestration can
/// resume work, expose status, and prove lifecycle transitions over time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ScanJob {
    /// Gives every scan attempt a durable identity for queue processing, audit
    /// logging, and report generation.
    pub id: Uuid,
    /// Associates the job with the case whose viewing key and evidence scope it
    /// is allowed to operate on.
    pub case_id: Uuid,
    /// Tells the orchestrator which adapter pipeline to invoke without embedding
    /// chain-specific state into the job runner itself.
    pub chain: ChainId,
    /// Keeps the job pinned to the intended network so a valid key on one network
    /// cannot be scanned against another by accident.
    pub network: Network,
    /// Publishes the current lifecycle stage so API clients and operators can
    /// observe progress and reason about failures.
    pub status: ScanJobStatus,
    /// Records when the job entered the system for SLA measurement, audit trails,
    /// and queue aging analysis.
    pub created_at: DateTime<Utc>,
    /// Records the latest state change so stuck jobs and recent failures are easy
    /// to identify without diffing audit logs.
    pub updated_at: DateTime<Utc>,
}

/// Represents the top-level compliance case because every scan, key import, and
/// report must remain anchored to an explicit investigatory scope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct Case {
    /// Gives the case a durable identifier that all downstream artifacts and
    /// audit records can reference consistently.
    pub id: Uuid,
    /// Ties the case to a workspace so tenant isolation and access control can be
    /// enforced above the raw database layer.
    pub workspace_id: Uuid,
    /// Records which chain the case is authorized to inspect so cross-chain data
    /// is never mixed under one ambiguous identifier.
    pub chain: ChainId,
    /// Records the network context to prevent test or local evidence from being
    /// confused with production activity.
    pub network: Network,
    /// Leaves room for case-level workflow state without forcing the schema to
    /// mirror the lower-level scan job lifecycle exactly.
    pub status: String,
    /// Records when the case was opened for auditability and operational aging.
    pub created_at: DateTime<Utc>,
    /// Records the last case-level mutation so external APIs can surface recent
    /// state changes without scanning audit tables.
    pub updated_at: DateTime<Utc>,
}
