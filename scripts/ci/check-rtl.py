# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""No left/right-only layout in app code, so Arabic and Hebrew mirror correctly.

Flags `EdgeInsets.only(left:/right:)`, `EdgeInsets.fromLTRB`, `Positioned(left:/right:)`,
`BorderRadius.only(topLeft: ...)`, `Alignment.centerLeft` and the other side alignments, and `TextAlign.left/right`
in app/**/lib. Use the Directional variants (`EdgeInsetsDirectional`, `PositionedDirectional`, `AlignmentDirectional`,
`TextAlign.start/end`). A gradient's `begin:`/`end:` is decorative and stays physical. A site that must stay physical
(picture coordinates, canvas drawing) carries an `rtl-ok` comment on its line or the line above.

Usage: python3 -I scripts/ci/check-rtl.py [app-root]   (defaults to the repo's app/)
"""

import pathlib
import re
import sys

ALIGN = re.compile(r"\bAlignment\.(?:center|top|bottom)(?:Left|Right)\b")
GRADIENT_SIDE = re.compile(r"^\s*(?:begin|end):")
PLAIN = re.compile(r"\bEdgeInsets\.fromLTRB\(|\bTextAlign\.(?:left|right)\b")
CALLS = {
    "EdgeInsets.only(": {"left", "right"},
    "Positioned(": {"left", "right"},
    "BorderRadius.only(": {"topLeft", "topRight", "bottomLeft", "bottomRight"},
}
NAMED_ARG = re.compile(r"(\w+)\s*:")


def _call_args(src: str, start: int) -> list[str]:
    """Top-level named arguments of the call whose `(` ends just before `start`."""
    depth, i, names = 1, start, []
    while i < len(src) and depth:
        c = src[i]
        if c in "'\"":
            end = src.find(c, i + 1)
            i = len(src) if end < 0 else end + 1
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
        elif depth == 1:
            m = NAMED_ARG.match(src, i)
            if m and not (src[i - 1].isalnum() or src[i - 1] in "_."):
                names.append(m.group(1))
                i = m.end()
                continue
        i += 1
    return names


def hits(src: str) -> list[tuple[int, str]]:
    """(line number, matched text) for each left/right-only use not excused by `rtl-ok`."""
    found = [(m.start(), m.group(0)) for m in PLAIN.finditer(src)]
    for m in ALIGN.finditer(src):
        line_start = src.rfind("\n", 0, m.start()) + 1
        if not GRADIENT_SIDE.match(src[line_start : m.start()]):
            found.append((m.start(), m.group(0)))
    for call, sides in CALLS.items():
        for m in re.finditer(r"(?<![\w.])" + re.escape(call), src):
            if sides & set(_call_args(src, m.end())):
                found.append((m.start(), call.rstrip("(")))
    lines = src.split("\n")
    out = []
    for pos, text in sorted(found):
        n = src.count("\n", 0, pos)
        if "rtl-ok" in lines[n] or (n > 0 and "rtl-ok" in lines[n - 1]):
            continue
        out.append((n + 1, text))
    return out


def main() -> int:
    root = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else pathlib.Path(__file__).resolve().parents[2] / "app")
    bad = 0
    for f in sorted(root.rglob("*.dart")):
        rel = f.relative_to(root).as_posix()
        if "/lib/" not in "/" + rel or "/gen/" in rel:
            continue
        for line, text in hits(f.read_text(encoding="utf-8")):
            print(f"::error file=app/{rel},line={line}::{text} is left/right-only; use the Directional variant "
                  "(or mark a site that must stay physical with an rtl-ok comment)")
            bad += 1
    if bad:
        return 1
    print("rtl: every app layout follows the text direction")
    return 0


if __name__ == "__main__":
    sys.exit(main())
