#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Manual image install-page-form: the install page built the way a release builds it, filled with sample
# values in headless Chrome, setup code blurred (in the code box and in the generated text).
# Usage: scripts/manual/install_page_shot.sh   (CHROME=<path to chrome or msedge> to override)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CHROME="${CHROME:-/c/Program Files/Google/Chrome/Application/chrome.exe}"
TMP="$(mktemp -d)"
trap 'rm -rf -- "$TMP" 2>/dev/null || true' EXIT
# Same placeholder URL the installer self-test uses; the page only needs a pinned-tag shape.
bash "$ROOT/scripts/release/build-install-page.sh" "$ROOT/deploy/install.sh" \
  "https://raw.githubusercontent.com/owner/repo/v1.6.0/install.sh" "$TMP" >/dev/null
python -I - "$TMP/index.html" "$TMP/shot.html" <<'PY'
import sys
page = open(sys.argv[1], encoding="utf-8").read()
fill = """<script>
addEventListener("load", () => {
  const set = (id, v) => { const e = document.getElementById(id); e.value = v; e.dispatchEvent(new Event("input")); };
  set("domain", "example.co.kr"); set("email", "owner@example.co.kr");
  for (const id of ["check-os", "check-fw"]) { const e = document.getElementById(id); e.checked = true; e.dispatchEvent(new Event("change")); }
  document.getElementById("make").click();
  for (const id of ["code", "output"]) document.getElementById(id).style.filter = "blur(6px)";
});
</script></body>"""
open(sys.argv[2], "w", encoding="utf-8").write(page.replace("</body>", fill))
PY
win() { cygpath -w "$1" 2>/dev/null || echo "$1"; }
"$CHROME" --headless --disable-gpu --hide-scrollbars --user-data-dir="$(win "$TMP/profile")" \
  --force-device-scale-factor=2 --window-size=820,1780 --virtual-time-budget=3000 \
  --screenshot="$(win "$TMP/shot.png")" "file:///$(cygpath -m "$TMP/shot.html" 2>/dev/null || echo "$TMP/shot.html")"
# Chrome on Windows can return before the file is written: wait for it.
for _ in $(seq 60); do [ -s "$TMP/shot.png" ] && break; sleep 0.5; done
[ -s "$TMP/shot.png" ] || { echo "no screenshot from $CHROME" >&2; exit 1; }
sleep 1
# The page is bilingual (Korean with English subtitles): both manuals use the same image.
for lang in ko en; do
  mkdir -p "$ROOT/docs/manual/img/$lang"
  cp "$TMP/shot.png" "$ROOT/docs/manual/img/$lang/install-page-form.png"
done
python -I "$ROOT/scripts/manual/shrink_png.py" "$ROOT/docs/manual/img" 400
