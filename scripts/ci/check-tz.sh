#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# No hard-coded Korean time zone in backend code or migrations. Dates and hours follow the install's zone:
# util::install_tz() / util::local_today() in Rust, dahae_tz() / util::LOCAL_TODAY in SQL, domain::zone in pure code.
# The default zone lives only in domain::zone::DEFAULT and migration 0511. Tests (crates/*/tests) pin Korean
# behaviour on purpose and are not scanned. A line that must name the zone (a provider's wire format, a fixture)
# carries a `tz-ok` comment, or its file goes in ALLOWED with the reason.
# Usage: bash scripts/ci/check-tz.sh [root]   (root defaults to the repo)
set -euo pipefail
root="${1:-$(cd "$(dirname "$0")/../.." && pwd)}"
cd "$root"

PATTERN='Asia/Seoul|Asia::Seoul|\+09:00|\bKST\b'
ALLOWED=(
  backend/crates/domain/src/zone.rs             # the default zone itself
  backend/migrations/0511_install_tz.sql        # dahae_tz() falls back to the default zone
  backend/crates/api/src/settings_registry.rs   # install.tz options cover every shipped pack
  backend/crates/api/src/providers/toss_payments.rs # Toss reads and writes Korean wall-clock time
  backend/crates/api/src/providers/toss_jwe.rs  # Toss payout JWE fixtures carry +09:00 timestamps
)

hits=$(grep -rnEI "$PATTERN" backend/crates/*/src backend/migrations 2>/dev/null | grep -v 'tz-ok' || true)
for allowed in "${ALLOWED[@]}"; do
  hits=$(printf '%s\n' "$hits" | grep -v "^$allowed:" || true)
done
hits=$(printf '%s\n' "$hits" | sed '/^$/d')
if [ -n "$hits" ]; then
  printf '%s\n' "$hits" | while IFS= read -r line; do
    echo "::error file=${line%%:*}::hard-coded time zone; use the install zone (scripts/ci/check-tz.sh): ${line#*:}"
  done
  exit 1
fi
echo "time zone: no hard-coded zone outside the allowed list"
