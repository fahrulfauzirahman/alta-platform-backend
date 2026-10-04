CREATE TABLE IF NOT EXISTS platform_outbox (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  event_id UUID NOT NULL UNIQUE,
  tenant_id UUID NOT NULL,
  event_type TEXT NOT NULL,
  aggregate_type TEXT NOT NULL,
  aggregate_id UUID NOT NULL,
  payload JSONB NOT NULL,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  dispatched_at TIMESTAMPTZ,
  attempts INT NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_outbox_undispatched ON platform_outbox (created_at ASC) WHERE dispatched_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_outbox_tenant ON platform_outbox (tenant_id, created_at DESC);
