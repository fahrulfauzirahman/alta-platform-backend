CREATE TABLE IF NOT EXISTS platform_idempotency (
  tenant_id UUID NOT NULL,
  idempotency_key TEXT NOT NULL,
  resource_id UUID NOT NULL,
  status_code INT NOT NULL DEFAULT 201,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (tenant_id, idempotency_key)
);
