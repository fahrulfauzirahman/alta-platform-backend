-- 0005: idempotency body-mismatch detection + job lease index (forward-only, backward compatible).
-- request_hash is nullable so pre-0005 rows replay as before (no conflict).
ALTER TABLE platform_idempotency ADD COLUMN IF NOT EXISTS request_hash TEXT;
CREATE INDEX IF NOT EXISTS idx_jobs_lease ON platform_job_executions (status, leased_until, created_at) WHERE status IN ('pending', 'leased');
