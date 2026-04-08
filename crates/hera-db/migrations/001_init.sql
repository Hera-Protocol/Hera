CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE OR REPLACE FUNCTION set_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TABLE tenants (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    api_key_hash TEXT NOT NULL UNIQUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    email TEXT NOT NULL UNIQUE,
    display_name TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE workspaces (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE cases (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    chain TEXT NOT NULL CHECK (chain IN ('ZCASH', 'NAMADA')),
    network TEXT NOT NULL CHECK (network IN ('MAINNET', 'TESTNET', 'REGTEST')),
    status TEXT NOT NULL DEFAULT 'created',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE view_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL UNIQUE REFERENCES cases(id) ON DELETE CASCADE,
    chain TEXT NOT NULL CHECK (chain IN ('ZCASH', 'NAMADA')),
    key_ref TEXT NOT NULL,
    ciphertext BYTEA NOT NULL,
    nonce BYTEA NOT NULL,
    encrypted_data_key BYTEA NOT NULL,
    birthday_height BIGINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
    -- No plaintext column exists here by design. Hera never persists raw viewing
    -- keys in the database, even temporarily.
);

CREATE TABLE chain_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    chain TEXT NOT NULL CHECK (chain IN ('ZCASH', 'NAMADA')),
    account_ref TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE scan_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    chain TEXT NOT NULL CHECK (chain IN ('ZCASH', 'NAMADA')),
    network TEXT NOT NULL CHECK (network IN ('MAINNET', 'TESTNET', 'REGTEST')),
    status TEXT NOT NULL,
    failure_reason TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE scan_checkpoints (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    chain TEXT NOT NULL CHECK (chain IN ('ZCASH', 'NAMADA')),
    checkpoint_kind TEXT NOT NULL,
    checkpoint_value BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (case_id, chain, checkpoint_kind)
);

CREATE TABLE raw_chain_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    chain TEXT NOT NULL CHECK (chain IN ('ZCASH', 'NAMADA')),
    txid TEXT NOT NULL,
    payload JSONB NOT NULL,
    observed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE owned_notes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    chain TEXT NOT NULL CHECK (chain IN ('ZCASH', 'NAMADA')),
    txid TEXT NOT NULL,
    note_ref TEXT NOT NULL UNIQUE,
    asset_id TEXT,
    amount_raw TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE canonical_events (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id UUID NOT NULL UNIQUE,
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    chain TEXT NOT NULL CHECK (chain IN ('ZCASH', 'NAMADA')),
    network TEXT NOT NULL CHECK (network IN ('MAINNET', 'TESTNET', 'REGTEST')),
    txid TEXT NOT NULL,
    block_height BIGINT NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    -- event_type is TEXT with a CHECK instead of a Postgres enum so widening the
    -- allowed set is a constraint update, not a database type rewrite.
    event_type TEXT NOT NULL CHECK (event_type IN ('SHIELD', 'RECEIVE', 'SEND', 'UNSHIELD', 'FEE')),
    asset_symbol TEXT NOT NULL,
    asset_id TEXT NOT NULL,
    asset_decimals SMALLINT NOT NULL,
    -- amount is TEXT to preserve the exact decimal representation emitted by the
    -- normalization layer. We never want the database to silently round values.
    amount TEXT NOT NULL,
    counterparty_visibility TEXT NOT NULL CHECK (counterparty_visibility IN ('KNOWN', 'UNKNOWN', 'PARTIAL')),
    counterparty_value TEXT,
    memo_present BOOLEAN NOT NULL DEFAULT FALSE,
    memo_hash TEXT,
    evidence_refs JSONB NOT NULL DEFAULT '[]'::JSONB,
    provenance_source TEXT NOT NULL,
    provenance_pool TEXT,
    scan_version TEXT NOT NULL,
    notes JSONB NOT NULL DEFAULT '[]'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL UNIQUE REFERENCES cases(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    json_s3_key TEXT,
    pdf_s3_key TEXT,
    json_sha256 TEXT,
    pdf_sha256 TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE report_signatures (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    report_id UUID NOT NULL REFERENCES reports(id) ON DELETE CASCADE,
    algorithm TEXT NOT NULL,
    public_key_hex TEXT NOT NULL,
    signature_hex TEXT NOT NULL,
    signed_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_id UUID NOT NULL,
    action TEXT NOT NULL,
    resource_id UUID NOT NULL,
    resource_type TEXT NOT NULL,
    ip_addr TEXT,
    metadata JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- no UPDATE or DELETE permitted on this table. Enforce via row-level security in prod.

CREATE INDEX idx_workspaces_tenant_id ON workspaces (tenant_id);
CREATE INDEX idx_cases_workspace_id ON cases (workspace_id);
CREATE INDEX idx_scan_jobs_case_id ON scan_jobs (case_id);
CREATE INDEX idx_scan_checkpoints_case_chain ON scan_checkpoints (case_id, chain);
CREATE INDEX idx_raw_chain_events_case_id ON raw_chain_events (case_id);
CREATE INDEX idx_owned_notes_case_id ON owned_notes (case_id);
CREATE INDEX idx_canonical_events_case_id ON canonical_events (case_id);
CREATE INDEX idx_canonical_events_case_timestamp ON canonical_events (case_id, timestamp);
CREATE INDEX idx_reports_case_id ON reports (case_id);
CREATE INDEX idx_report_signatures_report_id ON report_signatures (report_id);
CREATE INDEX idx_audit_logs_resource ON audit_logs (resource_type, resource_id);

CREATE TRIGGER trg_tenants_updated_at
BEFORE UPDATE ON tenants
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_users_updated_at
BEFORE UPDATE ON users
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_workspaces_updated_at
BEFORE UPDATE ON workspaces
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_cases_updated_at
BEFORE UPDATE ON cases
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_view_keys_updated_at
BEFORE UPDATE ON view_keys
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_chain_accounts_updated_at
BEFORE UPDATE ON chain_accounts
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_scan_jobs_updated_at
BEFORE UPDATE ON scan_jobs
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_scan_checkpoints_updated_at
BEFORE UPDATE ON scan_checkpoints
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_raw_chain_events_updated_at
BEFORE UPDATE ON raw_chain_events
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_owned_notes_updated_at
BEFORE UPDATE ON owned_notes
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_canonical_events_updated_at
BEFORE UPDATE ON canonical_events
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_reports_updated_at
BEFORE UPDATE ON reports
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_report_signatures_updated_at
BEFORE UPDATE ON report_signatures
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
