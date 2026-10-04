# Operations

- Migrations run from empty DB via `./scripts/migrate.sh`.
- Worker restart-safe via `FOR UPDATE SKIP LOCKED`.
- Logs are JSON with request_id; secrets never logged.
- Systemd units in `deploy/systemd`.
