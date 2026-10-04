# Architecture

- `kernel`: product-neutral types (ids, errors, pagination, RequestContext). No Axum/SQLx.
- `application`: use cases + ports (Outbox, Idempotency, Jobs, ReferenceItems).
- `infrastructure`: Axum adapters, SQLx repos, outbox publisher, config, observability.
- `apps/api`, `apps/worker`: composition roots only.

Dependency direction: api/worker -> application -> kernel; infrastructure -> kernel; infrastructure implements application ports.
