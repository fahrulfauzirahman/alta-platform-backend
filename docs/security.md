# Security

- Tenant isolation enforced at SQL level (`WHERE tenant_id = $1`); all reference/item and idempotency paths are tenant-scoped; SSE filters by `tenant_id` and drops events with missing tenant (`unwrap_or(false)`).
- Auth modes:
  - `AUTH_HS256_SECRET` set: API requires `Authorization: Bearer <HS256 JWT>` with claims `tid` (tenant uuid), optional `aid` (actor uuid), `exp`. Verified in `infrastructure::auth::verify_bearer` (leeway 0). If `X-Tenant-Id` is also sent it must match `tid` else 403. Use for Spoora/direct exposure.
  - `AUTH_HS256_SECRET` empty (local/test): trusts `X-Tenant-Id` (and optional `X-Actor-Id`, `X-Request-Id`) headers. Do not expose directly — put behind an authenticating proxy that sets `X-Tenant-Id` from verified identity. Startup logs a warning in this mode.
  - `StaticSessionService` remains test-only; no token issuance in foundation (issue JWTs from your identity provider).
- Idempotency-Key (non-empty, max 128 chars, tenant-scoped) prevents duplicate creates, including under 30-way concurrency (reserve-first in same transaction). Same key + different title hash returns `IDEMPOTENCY_CONFLICT` 409 with no second resource/outbox (migration 0005 `request_hash`, nullable for pre-0005 compat). Same key + same payload replays original with 200.
- Pagination cursor is opaque keyset `(created_at DESC, id DESC)` encoded as `<micros>:<uuid>`; invalid cursors return `VALIDATION_FAILED` 422 fail-closed.
- Rate limiting: in-memory per-tenant fixed window on `POST /v1/reference-items` (`RATE_LIMIT_PER_MIN`, default 200/min). Exceeding returns `RATE_LIMITED` 429. Single-instance limit; put a gateway limit in front for multi-instance strictness.
- Error responses use ErrorCode map with `request_id`; messages sanitized (`postgres://` redacted); `readyz` returns generic `not_ready` without DB details.
- Request bodies limited to 1MB (`DefaultBodyLimit::max`); titles validated 1..200 chars both in application and SQL `CHECK`. HTTP observability: `TraceLayer`, `X-Request-Id` set/propagate; CORS deny-by-default unless `ALLOWED_ORIGINS` is set (explicit allowlist, `Authorization`/`Idempotency-Key` etc. allowed).
- DB pool hardened: max 10 conns, `acquire_timeout` 5s, idle 60s, max lifetime 30min.
- Containers run as non-root `app` with `HEALTHCHECK` on `/healthz`; `security.yml` runs cargo-audit/deny (make blocking in CI).
