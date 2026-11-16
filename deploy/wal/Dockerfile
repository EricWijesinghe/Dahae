# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Postgres 17 with pgBackRest, for WAL archiving and base backups (see docs/DEPLOY.md, "WAL archiving").
# The stock postgres:17 image stays the default; this image is used only through docker-compose.wal.yml.
FROM postgres:17
RUN apt-get update \
 && apt-get install -y --no-install-recommends pgbackrest \
 && rm -rf /var/lib/apt/lists/*
# Runs once, on the first start of an empty data volume (creates the pg_stat_statements extension).
COPY init/ /docker-entrypoint-initdb.d/
