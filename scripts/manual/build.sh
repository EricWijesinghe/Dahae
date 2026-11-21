#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Builds the manual book PDFs from docs/manual/<lang>/*.md:
#   targets/manual/Dahae-Manual-ko.pdf and targets/manual/Dahae-Manual-en.pdf
# Usage: scripts/manual/build.sh [YYYY-MM-DD] [ko|en ...]     (date on the cover; default today)
#        MANUAL_OUT=/some/dir MANUAL_COPY_TO=/d/scrub/handover scripts/manual/build.sh 2026-11-21
# Needs python, node, Playwright with Chromium (SHOT_PLAYWRIGHT, as scripts/store/frame.js) and
# pdftotext (poppler; ships with Git for Windows). Printing is light, so it does not take a build slot.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/scripts/env.sh"

DATE="${1:-$(date +%F)}"
[ $# -gt 0 ] && shift
if [ $# -gt 0 ]; then LANGS=("$@"); else LANGS=(ko en); fi
OUT="${MANUAL_OUT:-$ROOT/targets/manual}"
command -v pdftotext >/dev/null || { echo "pdftotext not found (install poppler, or use Git Bash)" >&2; exit 1; }
python -I "$ROOT/scripts/release/md2html.py" --self-check
mkdir -p "$OUT"

for lang in "${LANGS[@]}"; do
  frag="$OUT/work-$lang"
  rm -rf "$frag"
  mkdir -p "$frag"
  for md in "$ROOT/docs/manual/$lang"/[0-9][0-9]-*.md; do
    python -I "$ROOT/scripts/release/md2html.py" --fragment "$md" "$frag/$(basename "${md%.md}").html"
  done
  node "$ROOT/scripts/manual/book.js" "$lang" "$DATE" "$frag" "$OUT/Dahae-Manual-$lang.pdf"
  [ -n "${MANUAL_KEEP_WORK:-}" ] || rm -rf "$frag"
  if [ -n "${MANUAL_COPY_TO:-}" ]; then
    mkdir -p "$MANUAL_COPY_TO"
    cp "$OUT/Dahae-Manual-$lang.pdf" "$MANUAL_COPY_TO/"
  fi
done
