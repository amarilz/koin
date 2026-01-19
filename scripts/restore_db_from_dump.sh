#!/usr/bin/env bash
set -euo pipefail

# Restore a SQL dump file into a Postgres container database.
# Usage:
#   ./scripts/restore_db_from_dump.sh /path/to/dump.sql [container_name]
# Environment variables (optional):
#   DB_NAME (default: mydb)
#   DB_USER (default: myuser)
#   DB_PASS (default: mypassword)
#   APP_CONTAINER (optional): name of the app container to stop during restore
#   STOP_APP (optional): if set to "true" and APP_CONTAINER provided, stop it before restore and start it after
# Notes:
# - The script will drop and recreate the target database before restoring.
# - The dump file is streamed into `psql` inside the Postgres container.

DUMP_FILE=${1:-}
CONTAINER_NAME=${2:-postgres}
DB_NAME=${DB_NAME:-koin_db}
DB_USER=${DB_USER:-koin_user}
DB_PASS=${DB_PASS:-koin_password}
APP_CONTAINER=${APP_CONTAINER:-}
STOP_APP=${STOP_APP:-false}

if [[ -z "$DUMP_FILE" ]]; then
  echo "Usage: $0 /path/to/dump.sql [container_name]"
  exit 1
fi

if [[ ! -f "$DUMP_FILE" ]]; then
  echo "[ERROR] Dump file not found: $DUMP_FILE" >&2
  exit 2
fi

echo "[INFO] Restoring '$DUMP_FILE' into database '$DB_NAME' on container '$CONTAINER_NAME'"

# Verify container is running
if ! docker ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
  echo "[ERROR] Container '$CONTAINER_NAME' is not running." >&2
  exit 3
fi

# Optionally stop app container to avoid connections during restore
if [[ "$STOP_APP" == "true" && -n "$APP_CONTAINER" ]]; then
  if docker ps --format '{{.Names}}' | grep -q "^${APP_CONTAINER}$"; then
    echo "[INFO] Stopping app container '$APP_CONTAINER'..."
    docker stop "$APP_CONTAINER" || true
  fi
fi

export PGPASSWORD="$DB_PASS"

# Drop and recreate database
echo "[INFO] Dropping and recreating database '$DB_NAME'..."
docker exec -i -e PGPASSWORD="$DB_PASS" "$CONTAINER_NAME" psql -U "$DB_USER" -d postgres -v ON_ERROR_STOP=1 -c "DROP DATABASE IF EXISTS \"$DB_NAME\";"
docker exec -i -e PGPASSWORD="$DB_PASS" "$CONTAINER_NAME" psql -U "$DB_USER" -d postgres -v ON_ERROR_STOP=1 -c "CREATE DATABASE \"$DB_NAME\";"

# Restore dump
echo "[INFO] Importing dump..."

# If the dump file looks like plain SQL (ends with .sql), use psql; otherwise use pg_restore for binary formats
lcfile=$(echo "$DUMP_FILE" | awk -F. '{print tolower($NF)}')
if [[ "$lcfile" == "sql" ]]; then
  if docker exec -i -e PGPASSWORD="$DB_PASS" "$CONTAINER_NAME" psql -U "$DB_USER" -d "$DB_NAME" -v ON_ERROR_STOP=1 < "$DUMP_FILE"; then
    echo "[OK] Restore completed successfully (psql)."
  else
    echo "[ERROR] Restore (psql) failed." >&2
    exit 4
  fi
else
  # Use pg_restore; stream the dump into the container's stdin
  if docker exec -i -e PGPASSWORD="$DB_PASS" "$CONTAINER_NAME" pg_restore -U "$DB_USER" -d "$DB_NAME" -v < "$DUMP_FILE"; then
    echo "[OK] Restore completed successfully (pg_restore)."
  else
    echo "[ERROR] Restore (pg_restore) failed." >&2
    exit 4
  fi
fi

# Optionally start app container again
if [[ "$STOP_APP" == "true" && -n "$APP_CONTAINER" ]]; then
  echo "[INFO] Starting app container '$APP_CONTAINER'..."
  docker start "$APP_CONTAINER" || true
fi

exit 0
