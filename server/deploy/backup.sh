#!/usr/bin/env bash
# Online backup of the two server databases (docs/v2/DEPLOY.md).
#
# Uses `sqlite3 <db> ".backup '<dest>'"`, which is safe to run against a live
# database (SQLite's own backup API, consistent even mid-write) and needs no
# downtime.
#
# Usage:
#   DATA_DIR=/data BACKUP_DIR=/backups RETENTION_DAYS=14 backup.sh
#
# Defaults assume the container layout (SPARAGNE_DATA_DIR=/data). For a
# bare-metal install pass the real paths, e.g.:
#   DATA_DIR=/var/lib/sparagne BACKUP_DIR=/var/backups/sparagne backup.sh
#
# Compose: this script and the sqlite3 CLI ship inside the sparagne image
# (see server/Dockerfile), and server/deploy/compose.yml bind-mounts a
# ./backups host directory at /backups, so from server/deploy/ run:
#   docker compose exec sparagne backup.sh
# The dump then lands directly on the host under ./backups/<timestamp>/.
set -euo pipefail

DATA_DIR="${DATA_DIR:-/data}"
BACKUP_DIR="${BACKUP_DIR:-/backups}"
RETENTION_DAYS="${RETENTION_DAYS:-14}"

if ! command -v sqlite3 >/dev/null 2>&1; then
    echo "backup.sh: sqlite3 is not installed" >&2
    exit 1
fi

for db in vaults.sqlite server.sqlite; do
    if [ ! -f "$DATA_DIR/$db" ]; then
        echo "backup.sh: $DATA_DIR/$db not found" >&2
        exit 1
    fi
done

timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
dest="$BACKUP_DIR/$timestamp"
mkdir -p "$dest"

for db in vaults.sqlite server.sqlite; do
    echo "backup.sh: backing up $db"
    sqlite3 "$DATA_DIR/$db" ".backup '$dest/$db'"
    sqlite3 "$dest/$db" "PRAGMA integrity_check;" | grep -qx ok || {
        echo "backup.sh: integrity check failed for $db" >&2
        exit 1
    }
done

echo "backup.sh: wrote $dest"

if [ "$RETENTION_DAYS" -gt 0 ] 2>/dev/null; then
    find "$BACKUP_DIR" -mindepth 1 -maxdepth 1 -type d -mtime "+$RETENTION_DAYS" -print -exec rm -rf {} + \
        | sed 's/^/backup.sh: pruning /'
fi
