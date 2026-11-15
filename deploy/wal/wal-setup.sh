#!/usr/bin/env bash
# Turns on WAL archiving once the S3 settings are in .env. Engineer CLI, run from the install folder.
# Order matters: archive_mode goes on first (a restart), then the stanza is created and checked.
# Usage: bash wal/wal-setup.sh
set -euo pipefail
dc() { docker compose -f docker-compose.yml -f wal/docker-compose.wal.yml --env-file .env "$@"; }
env_get() { { grep -E "^$1=" .env 2>/dev/null || true; } | tail -n1 | cut -d= -f2-; }

for k in WAL_S3_BUCKET WAL_S3_ENDPOINT WAL_S3_KEY WAL_S3_SECRET WAL_CIPHER_PASS; do
  [ -n "$(env_get "$k")" ] || { echo "missing $k in .env" >&2; exit 1; }
done

if grep -q '^WAL_ARCHIVE_MODE=' .env; then
  sed -i 's/^WAL_ARCHIVE_MODE=.*/WAL_ARCHIVE_MODE=on/' .env
else
  echo 'WAL_ARCHIVE_MODE=on' >> .env
fi
dc up -d --build db
dc exec -T --user postgres db pgbackrest --stanza=malkkeum stanza-create
dc exec -T --user postgres db pgbackrest --stanza=malkkeum check
dc exec -T --user postgres db pgbackrest --stanza=malkkeum --type=full backup
echo "WAL archiving is on. Schedule wal/base-backup.sh weekly."
