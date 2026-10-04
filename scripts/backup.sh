#!/bin/bash
set -euo pipefail
: "${DATABASE_URL:?set DATABASE_URL}"
: "${BACKUP_DIR:=./backups}"
mkdir -p "$BACKUP_DIR"
OUT="$BACKUP_DIR/alta_$(date +%F_%H%M%S).dump"
echo "backing up to $OUT"
pg_dump -Fc -f "$OUT" "$DATABASE_URL"
echo "backup done: $OUT"
echo "verify with: pg_restore -l $OUT | head"
