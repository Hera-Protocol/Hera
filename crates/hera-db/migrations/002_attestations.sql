CREATE TABLE attestation_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    proof_type TEXT NOT NULL CHECK (proof_type IN (
        'THRESHOLD_RECEIVED', 'NO_BLOCKLIST_EXPOSURE', 'RISK_BELOW_THRESHOLD'
    )),
    status TEXT NOT NULL,
    failure_reason TEXT,
    parameters JSONB NOT NULL DEFAULT '{}'::JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE attestations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    case_id UUID NOT NULL REFERENCES cases(id) ON DELETE CASCADE,
    job_id UUID NOT NULL REFERENCES attestation_jobs(id) ON DELETE CASCADE,
    proof_type TEXT NOT NULL,
    proof_s3_key TEXT NOT NULL,
    proof_sha256 TEXT NOT NULL,
    public_inputs_json JSONB NOT NULL,
    srs_version TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_attestation_jobs_case_id ON attestation_jobs (case_id);
CREATE INDEX idx_attestations_case_id ON attestations (case_id);
CREATE INDEX idx_attestations_job_id ON attestations (job_id);

CREATE TRIGGER trg_attestation_jobs_updated_at
BEFORE UPDATE ON attestation_jobs
FOR EACH ROW EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_attestations_updated_at
BEFORE UPDATE ON attestations
FOR EACH ROW EXECUTE FUNCTION set_updated_at();
