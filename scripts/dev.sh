#!/bin/bash
set -euo pipefail
docker compose -f deploy/docker-compose.yml up -d postgres
echo "postgres up; run: cargo run -p alta-api"
