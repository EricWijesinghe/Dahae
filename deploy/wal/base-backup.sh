#!/usr/bin/env bash
# Weekly full base backup to the WAL bucket, then a check that archiving still works. Engineer CLI, run
# from the install folder (cron): bash wal/base-backup.sh
set -euo pipefail
dc() { docker compose -f docker-compose.yml -f wal/docker-compose.wal.yml --env-file .env "$@"; }
dc exec -T --user postgres db pgbackrest --stanza=malkkeum --type=full backup
dc exec -T --user postgres db pgbackrest --stanza=malkkeum check
