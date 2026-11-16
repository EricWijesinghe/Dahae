#!/usr/bin/env bash
# Turns on WAL archiving once the S3 settings are in .env. Engineer CLI, run from the install folder.
# Order matters: the pgBackRest image comes up healthy with archiving still off, then WAL_ARCHIVE_MODE goes on
# (a restart), then the stanza is created, checked and backed up. If that fails, archiving goes back off, so a
# broken archive_command never piles WAL up on the database disk.
# Usage: bash wal/wal-setup.sh
set -euo pipefail
dc() { docker compose -f docker-compose.yml -f wal/docker-compose.wal.yml --env-file .env "$@"; }
env_get() { { grep -E "^$1=" .env 2>/dev/null || true; } | tail -n1 | cut -d= -f2-; }
set_wal_mode() {
  if grep -q '^WAL_ARCHIVE_MODE=' .env; then
    sed -i "s/^WAL_ARCHIVE_MODE=.*/WAL_ARCHIVE_MODE=$1/" .env
  else
    echo "WAL_ARCHIVE_MODE=$1" >> .env
  fi
}

for k in WAL_S3_BUCKET WAL_S3_ENDPOINT WAL_S3_KEY WAL_S3_SECRET WAL_CIPHER_PASS; do
  [ -n "$(env_get "$k")" ] || { echo "missing $k in .env" >&2; exit 1; }
done

set_wal_mode off
dc up -d --build --wait db
set_wal_mode on
if dc up -d --wait db \
  && dc exec -T --user postgres db pgbackrest --stanza=malkkeum stanza-create \
  && dc exec -T --user postgres db pgbackrest --stanza=malkkeum check \
  && dc exec -T --user postgres db pgbackrest --stanza=malkkeum --type=full backup; then
  if [ -x /usr/local/bin/dahae ]; then /usr/local/bin/dahae write-cron; fi
  echo "WAL archiving is on. The weekly base backup is scheduled by dahae (or run wal/base-backup.sh by hand)."
else
  set_wal_mode off
  dc up -d --wait db >/dev/null || true
  echo "WAL archiving failed and was turned back off. Check the messages above, then run this again." >&2
  exit 1
fi
