# alta-platform-backend

Rust platform foundation (Axum + SQLx + Postgres) for ALTA.

- 2 apps: `api`, `worker`
- 3 crates: `kernel`, `application`, `infrastructure`
- Reference vertical slice: `ReferenceItem` with transactional outbox + SSE.

## Quickstart

```bash
cp .env.example .env
docker compose -f deploy/docker-compose.yml up -d postgres
./scripts/migrate.sh
cargo run -p alta-api
cargo run -p alta-worker
```

Endpoints: `GET /healthz`, `GET /readyz`, `GET /v1/session`, `GET /v1/reference-items`, `POST /v1/reference-items`, `GET /v1/events`.

See `docs/architecture.md`.
