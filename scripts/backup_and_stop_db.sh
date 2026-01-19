#!/usr/bin/env bash
set -euo pipefail

# Script per fare pg_dump dal container postgres e poi fermarlo.
# Uso:
#   BACKUP_DIR=/path/to/backups ./scripts/backup_and_stop_db.sh [container_name]
# Esempio:
#   BACKUP_DIR=$HOME/db_backups ./scripts/backup_and_stop_db.sh postgres

CONTAINER_NAME=${1:-postgres}

# Carica .env se presente nella root del progetto.
script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
project_root=$(cd "$script_dir/.." && pwd)
if [[ -f "$project_root/.env" ]]; then
  set -a
  . "$project_root/.env"
  set +a
fi

BACKUP_DIR=${BACKUP_BASE_PATH}

# Credenziali di default (coerenti con compose_prod.yaml)
DB_NAME=${DB_NAME:-koin_db}
DB_USER=${DB_USER:-koin_user}
DB_PASS=${DB_PASS:-koin_password}

mkdir -p "$BACKUP_DIR"

timestamp=$(date +"%Y%m%d_%H%M%S")
# Use binary custom format (.dump)
outfile="$BACKUP_DIR/${CONTAINER_NAME}_${DB_NAME}_backup_${timestamp}.dump"

echo "[INFO] Backup di $DB_NAME dal container '$CONTAINER_NAME' in: $outfile"

# Controlla che il container esista
if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
  echo "[WARN] Container '$CONTAINER_NAME' non è in esecuzione. Uscita senza fare il dump."
  exit 1
fi

# Esegui pg_dump in formato custom (-F c) dentro il container e redirigi l'output (binario) su file host
export PGPASSWORD="$DB_PASS"
if docker exec -i "$CONTAINER_NAME" pg_dump -U "$DB_USER" -d "$DB_NAME" -F c > "$outfile"; then
  echo "[OK] Dump completato: $outfile"
else
  echo "[ERROR] pg_dump fallito" >&2
  rm -f "$outfile" || true
  exit 2
fi

exit 0
