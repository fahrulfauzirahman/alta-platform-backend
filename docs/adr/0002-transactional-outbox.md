# ADR 0002: Transactional outbox

Write `reference_items` + `platform_outbox` in one Postgres transaction. Worker claims with SKIP LOCKED and marks dispatched_at.
