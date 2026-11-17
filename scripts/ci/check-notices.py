# Copyright (c) 2026 Eric Wijesinghe <EricWijesinghe@hotmail.com>. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""Checks that THIRD_PARTY_NOTICES.md names every Rust crate the api binary links and every Dart package the apps ship.

  cd backend && python3 -I ../scripts/ci/check-notices.py   # Rust: needs cargo; exit 1 when a crate is not listed
  cd app && flutter pub deps --json > deps.json
  python3 -I scripts/ci/check-notices.py --pub-deps deps.json --dart-only   # Dart: exit 1 when a shipped package is not listed

Without --pub-deps the Dart packages in app/pubspec.lock are only reported: pubspec.lock does not say which packages
are reachable from the shipped apps' dependencies, so the report may include dev-only packages.
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
NOTICES = REPO / "THIRD_PARTY_NOTICES.md"
ROOT_CRATE = "api"
TRIPLES = ["x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu"]  # the two image platforms (backend/deny.toml)
PUB_LOCK = REPO / "app" / "pubspec.lock"
SHIPPED_APPS = ["customer", "partner", "console"]  # app/apps/*, the three apps THIRD_PARTY_NOTICES.md covers


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


def non_dev_edges(package):
    """Dependencies of a package that are not also dev dependencies (`pub deps --no-dev` semantics)."""
    dev = set(package.get("devDependencies", []))
    return [dep for dep in package.get("dependencies", []) if dep not in dev]


def dart_shipped(deps_json):
    """(name, version) of every hosted package reachable from the shipped apps over non-dev edges.

    Input is `flutter pub deps --json` run in app/. Its --no-dev flag cannot be combined with --json, so the graph is walked here.
    """
    data = json.loads(deps_json)
    by_name = {p["name"]: p for p in data["packages"]}
    stack = []
    for app in SHIPPED_APPS:
        if app not in by_name:
            raise ValueError(f"shipped app {app} is missing from the pub deps output")
        stack += non_dev_edges(by_name[app])
    seen = set()
    while stack:
        name = stack.pop()
        if name in seen:
            continue
        if name not in by_name:
            raise ValueError(f"{name} is a dependency but missing from the pub deps output")
        seen.add(name)
        stack += non_dev_edges(by_name[name])
    return {(by_name[n]["name"], by_name[n]["version"]) for n in seen if by_name[n].get("source") == "hosted"}


def cargo_normal_crates():
    """What `cargo build -p api` (backend/Dockerfile) links: normal edges only, per image platform."""
    crates = set()
    for triple in TRIPLES:
        out = subprocess.run(["cargo", "tree", "-p", ROOT_CRATE, "-e", "normal", "--target", triple,
                              "--prefix", "none", "--locked"],
                             cwd=REPO / "backend", capture_output=True, text=True, check=True).stdout
        crates |= tree_crates(out.splitlines())
    return crates


def check_rust(notices):
    crates = cargo_normal_crates()
    missing = sorted(crates - notices)
    for name, version in missing:
        print(f"FAIL {name} {version}: in the API image but not in THIRD_PARTY_NOTICES.md")
    print(f"notices: {len(crates)} Rust crates in the API image, {len(missing)} missing")
    return len(missing)


def check_dart(notices, deps_path):
    shipped = dart_shipped(Path(deps_path).read_text(encoding="utf-8"))
    missing = sorted(shipped - notices)
    for name, version in missing:
        print(f"FAIL {name} {version}: shipped in the apps but not in THIRD_PARTY_NOTICES.md")
    print(f"dart: {len(shipped)} hosted packages reachable from the shipped apps, {len(missing)} missing")
    return len(missing)


def report_dart(notices):
    pub = pub_hosted(PUB_LOCK.read_text(encoding="utf-8")) - notices
    print(f"report only: {len(pub)} hosted pub packages in app/pubspec.lock are not in THIRD_PARTY_NOTICES.md")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--pub-deps", metavar="JSON", help="`flutter pub deps --json` output from app/; enforces the Dart part")
    parser.add_argument("--dart-only", action="store_true", help="skip the Rust check (needs --pub-deps)")
    args = parser.parse_args(argv)
    if args.dart_only and not args.pub_deps:
        parser.error("--dart-only needs --pub-deps")
    notices = listed(NOTICES.read_text(encoding="utf-8"))
    failures = 0
    if not args.dart_only:
        failures += check_rust(notices)
    if args.pub_deps:
        failures += check_dart(notices, args.pub_deps)
    else:
        report_dart(notices)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
