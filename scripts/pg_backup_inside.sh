#!/usr/bin/env bash
set -euo pipefail

# Script eseguito dentro al container backup: connette a Postgres e crea un dump in /backups

DB_HOST=${DB_HOST:-postgres}
DB_PORT=${DB_PORT:-5432}
DB_NAME=${DB_NAME:-koin_db}
DB_USER=${DB_USER:-koin_user}
DB_PASS=${DB_PASS:-koin_password}
BACKUP_DIR="/backups"

timestamp=$(date +"%Y%m%d_%H%M%S")
outfile="$BACKUP_DIR/postgres_${DB_NAME}_backup_${timestamp}.dump"

mkdir -p "$BACKUP_DIR"
export PGPASSWORD="$DB_PASS"

echo "[INFO] Eseguo pg_dump -> $outfile (host=$DB_HOST port=$DB_PORT user=$DB_USER)"
if pg_dump -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -F c -f "$outfile"; then
  echo "[OK] Dump creato: $outfile"
else
  echo "[ERROR] pg_dump fallito" >&2
  rm -f "$outfile" || true
  exit 1
fi

exit 0
