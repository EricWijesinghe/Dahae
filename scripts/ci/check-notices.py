# Copyright (c) 2026 Eric Wijesinghe <EricWijesinghe@hotmail.com>. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""Checks that THIRD_PARTY_NOTICES.md names every Rust crate the api binary links (normal dependencies, Linux triples).

  cd backend && python3 -I ../scripts/ci/check-notices.py   # needs cargo; exit 1 when a crate is not listed

The Dart packages in app/pubspec.lock are only reported: pubspec.lock does not say which packages are
reachable from the shipped apps' dependencies, so the report may include dev-only packages.
"""
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
NOTICES = REPO / "THIRD_PARTY_NOTICES.md"
ROOT_CRATE = "api"
TRIPLES = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"]  # the two image platforms (backend/deny.toml)
PUB_LOCK = REPO / "app" / "pubspec.lock"


TREE_LINE = re.compile(r"^(\S+) v(\S+?)(?: \(proc-macro\))?$")


def tree_crates(lines):
    """(name, version) from `cargo tree -p api -e normal --prefix none`. Local path members are skipped; a git source is refused."""
    crates = set()
    for line in lines:
        line = line.removesuffix(" (*)").strip()
        if not line:
            continue
        m = TREE_LINE.match(line)
        if m:
            crates.add((m.group(1), m.group(2)))
        elif "(http" in line or "(git" in line:
            raise ValueError(f"git source, list it by hand after review: {line}")
    return crates


def listed(text):
    """(name, version) from the first two cells of every table row."""
    rows = set()
    for line in text.splitlines():
        if line.startswith("|"):
            cells = [c.strip() for c in line.split("|")]
            if len(cells) > 3:
                rows.add((cells[1], cells[2]))
    return rows


def pub_hosted(lock_text):
    """(name, version) of every hosted package in a pubspec.lock."""
    out, name, version, hosted = set(), None, None, False
    for line in lock_text.splitlines():
        if re.match(r"^  \S+:$", line):
            name, version, hosted = line.strip()[:-1], None, False
        elif line == "    source: hosted":
            hosted = True
        elif hosted and (m := re.match(r'^    version: "(.*)"$', line)):
            out.add((name, m.group(1)))
    return out


def cargo_normal_crates():
    """What `cargo build -p api` (backend/Dockerfile) links: normal edges only, per image platform."""
    crates = set()
    for triple in TRIPLES:
        out = subprocess.run(["cargo", "tree", "-p", ROOT_CRATE, "-e", "normal", "--target", triple,
                              "--prefix", "none", "--locked"],
                             cwd=REPO / "backend", capture_output=True, text=True, check=True).stdout
        crates |= tree_crates(out.splitlines())
    return crates


def main():
    notices = listed(NOTICES.read_text(encoding="utf-8"))
    crates = cargo_normal_crates()
    missing = sorted(crates - notices)
    for name, version in missing:
        print(f"FAIL {name} {version}: in the API image but not in THIRD_PARTY_NOTICES.md")
    print(f"notices: {len(crates)} Rust crates in the API image, {len(missing)} missing")
    pub = pub_hosted(PUB_LOCK.read_text(encoding="utf-8")) - notices
    print(f"report only: {len(pub)} hosted pub packages in app/pubspec.lock are not in THIRD_PARTY_NOTICES.md")
    return 1 if missing else 0


if __name__ == "__main__":
    sys.exit(main())
