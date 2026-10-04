# Security

- Tenant isolation enforced at SQL level (`WHERE tenant_id = $1`).
- Idempotency-Key prevents duplicate creates.
- Error responses use ErrorCode map; messages sanitized.
- `security.yml` runs cargo-audit/deny.
