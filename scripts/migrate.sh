#!/bin/bash
set -euo pipefail
: "${DATABASE_URL:?set DATABASE_URL}"
cargo install sqlx-cli --no-default-features --features postgres 2>/dev/null || true
sqlx migrate run --source ./migrations
