#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Renders the manual book screenshots (docs/manual/img/<lang>/<id>.png) from the real screens with
# sample data, then trims and shrinks them. Flutter tests tagged manual_screenshots; never part of CI.
# Usage: scripts/manual/shots.sh                 (console, customer, partner)
#        scripts/manual/shots.sh console         (one app)
#        SHOT_ONLY=console-login,console-palette scripts/manual/shots.sh console
# Third-party sites (AWS, Cloudflare, PortOne, Toss, stores) are not rendered: see docs/manual/img/PLACEHOLDERS.md.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/env.sh"
FLUTTER="${FLUTTER:-flutter}"
OUT="$(cd "$ROOT/docs/manual" && mkdir -p img && cd img && (pwd -W 2>/dev/null || pwd))"

for app in ${*:-console customer partner}; do
  (
    cd "$ROOT/app/apps/$app"
    "$ROOT/scripts/build-slot.sh" "$FLUTTER" test test/manual_screenshots \
      --tags manual_screenshots \
      --dart-define=SHOT_OUT="$OUT" \
      --dart-define=SHOT_ONLY="${SHOT_ONLY:-}" 2>&1 | tail -n 25
  )
done
python -I "$ROOT/scripts/manual/shrink_png.py" "$OUT" 400
