# Copyright (c) 2026 Eric Wijesinghe <EricWijesinghe@hotmail.com>. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""Prints the CHANGELOG.md section of one version, used as the GitHub Release body (release.yml).

  python3 -I scripts/release/changelog-notes.py v1.5.0 [CHANGELOG.md]

Exit 1 when the version has no section or the section is empty, so a release never ships without notes.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]


def section(text, version):
    """Body of the `## [<version>]` section, without its heading. The tag's leading v is not part of the heading.

    A pre-release tag (v1.6.6-beta.1) without its own section uses its release's section ([1.6.6]), so a beta
    carries the notes of the release it leads to.
    """
    ver = version.removeprefix("v")
    lines = text.splitlines()

    def find(v):
        heading = re.compile(r"^## \[" + re.escape(v) + r"\](?: |$)")
        return next((i for i, line in enumerate(lines) if heading.match(line)), None)

    start = find(ver)
    if start is None and "-" in ver:
        start = find(ver.split("-", 1)[0])
    if start is None:
        raise ValueError(f"CHANGELOG has no section [{ver}]")
    end = next((i for i in range(start + 1, len(lines)) if lines[i].startswith("## [")), len(lines))
    body = "\n".join(lines[start + 1:end]).strip()
    if not body:
        raise ValueError(f"CHANGELOG section [{ver}] is empty")
    return body + "\n"


def main(argv):
    if len(argv) not in (2, 3):
        print("usage: changelog-notes.py <version> [CHANGELOG.md]", file=sys.stderr)
        return 2
    path = pathlib.Path(argv[2]) if len(argv) == 3 else ROOT / "CHANGELOG.md"
    try:
        sys.stdout.buffer.write(section(path.read_text(encoding="utf-8"), argv[1]).encode("utf-8"))
    except ValueError as e:
        print(f"::error::{e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
