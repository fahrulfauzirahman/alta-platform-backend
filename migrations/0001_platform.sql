CREATE EXTENSION IF NOT EXISTS "pgcrypto";

CREATE TABLE IF NOT EXISTS reference_items (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  tenant_id UUID NOT NULL,
  title TEXT NOT NULL CHECK (char_length(title) BETWEEN 1 AND 200),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS idx_reference_items_tenant_created ON reference_items (tenant_id, created_at DESC, id DESC);
