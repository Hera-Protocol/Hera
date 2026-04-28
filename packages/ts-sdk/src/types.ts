export type ChainId = "ZCASH" | "NAMADA";

export type Network = "MAINNET" | "TESTNET" | "REGTEST";

export type EventType = "SHIELD" | "RECEIVE" | "SEND" | "UNSHIELD" | "FEE";

export type CounterpartyVisibility = "KNOWN" | "UNKNOWN" | "PARTIAL";

export type ScanJobStatus =
  | "CREATED"
  | "KEY_VALIDATED"
  | "CHAIN_SYNCING"
  | "DETECTING_NOTES"
  | "CLASSIFYING_FLOWS"
  | "BUILDING_REPORT"
  | "SIGNED"
  | { FAILED: string };

export interface PaginationParams {
  limit?: number;
  offset?: number;
}

export interface PaginatedResponse<T> {
  items: T[];
  limit: number;
  offset: number;
}

export interface Asset {
  symbol: string;
  asset_id: string;
  decimals: number;
}

export interface Counterparty {
  visibility: CounterpartyVisibility;
  value: string | null;
}

export interface EventMemo {
  present: boolean;
  hash: string | null;
}

export interface EventProvenance {
  source: string;
  pool: string | null;
  scan_version: string;
}

export interface CanonicalEvent {
  event_id: string;
  case_id: string;
  chain: ChainId;
  network: Network;
  event_type: EventType;
  txid: string;
  block_height: number;
  timestamp: string;
  asset: Asset;
  amount: string;
  counterparty: Counterparty;
  memo: EventMemo;
  evidence_refs: string[];
  provenance: EventProvenance;
  notes: string[];
}

export interface CreateWorkspaceRequest {
  name: string;
}

export interface CreateWorkspaceResponse {
  id: string;
  name: string;
}

export interface WorkspaceSummaryResponse {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
}

export interface CreateCaseRequest {
  workspace_id: string;
  chain: ChainId;
  network: Network;
}

export interface CreateCaseResponse {
  id: string;
}

export interface CaseSummaryResponse {
  id: string;
  workspace_id: string;
  chain: ChainId;
  network: Network;
  case_status: string;
  scan_status: ScanJobStatus;
  created_at: string;
  updated_at: string;
}

export interface CaseDetailResponse {
  id: string;
  workspace_id: string;
  chain: ChainId;
  network: Network;
  case_status: string;
  scan_status: ScanJobStatus;
  created_at: string;
  updated_at: string;
  last_checkpoint: number | null;
}

export interface ImportViewingKeyRequest {
  raw_key: string;
  birthday_height?: number;
}

export interface ImportViewingKeyResponse {
  key_ref: string;
}

export interface ScanCaseResponse {
  job_id: string;
}

export interface GetCaseStatusResponse {
  status: ScanJobStatus;
  last_checkpoint: number | null;
}
