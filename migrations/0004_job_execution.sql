CREATE TABLE IF NOT EXISTS platform_job_executions (
  job_id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  job_type TEXT NOT NULL,
  payload JSONB NOT NULL DEFAULT '{}'::jsonb,
  status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','leased','done','failed')),
  leased_until TIMESTAMPTZ,
  attempts INT NOT NULL DEFAULT 0,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_jobs_pending ON platform_job_executions (status, created_at) WHERE status = 'pending';
CREATE TABLE IF NOT EXISTS platform_event_consumers (
  consumer TEXT PRIMARY KEY,
  last_event_id UUID,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
