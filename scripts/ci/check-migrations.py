#!/usr/bin/env python3
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

"""Migration safety gate. Fails when:

- two migrations share a number, or a file name does not match NNNN_snake_case.sql;
- a migration that exists on the base ref was edited, renamed or deleted (released migrations are
  immutable: sqlx stores a checksum per version, so an edit breaks every running database);
- a new migration creates a non-concurrent index, or adds a constraint without NOT VALID, on a big
  table (exempt when the same file creates the table);
- a CREATE INDEX CONCURRENTLY file lacks the `-- no-transaction` first line or holds more than one
  statement (sqlx sends a file as one simple query, which Postgres runs in an implicit transaction,
  and CONCURRENTLY refuses to run inside one);
- a new migration drops, renames or retypes something (DROP TABLE/COLUMN/TYPE/FUNCTION/TRIGGER/CONSTRAINT,
  RENAME, ALTER COLUMN ... TYPE, ALTER COLUMN ... SET NOT NULL) without a `-- contract: unused since vX.Y.Z`
  line naming a release at or below the last one (expand/contract: the release before must already run
  without it, so an automatic rollback to it still works). Objects the same file creates, and a
  constraint the same file adds again under its name, are exempt.

Usage: python3 -I scripts/ci/check-migrations.py [--base REF] [--dir backend/migrations]
Base: --base, else the newest v* tag, else origin/master, else master. The last release for the contract
rule is the newest v* tag reachable from the base.
Limits: statements are split on ';' after stripping -- comments, so a literal ';' or '--' inside a
string literal would confuse it; keep migrations free of both.
"""

import argparse
import pathlib
import re
import subprocess
import sys

BIG_TABLES = {
    "addresses", "bookings", "chat_messages", "chat_threads", "checkouts", "coupons", "ai_usage",
    "audit_log", "notifications", "ops_failures", "otp_codes", "payment_methods", "payments", "payouts",
    "pros", "push_tokens", "quotes", "refresh_tokens", "review_reports", "reviews", "settlements",
    "team_members", "uploads", "user_consents", "users",
}
NAME_RE = re.compile(r"^(\d{4})_[a-z0-9_]+\.sql$")
CREATE_INDEX_RE = re.compile(
    r"^CREATE\s+(?:UNIQUE\s+)?INDEX\s+(CONCURRENTLY\s+)?(?:IF\s+NOT\s+EXISTS\s+)?\S+\s+ON\s+(?:ONLY\s+)?(\w+)",
    re.IGNORECASE,
)
ALTER_TABLE_RE = re.compile(r"^ALTER\s+TABLE\s+(?:ONLY\s+)?(\w+)", re.IGNORECASE)
CREATE_TABLE_RE = re.compile(r"^CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(\w+)", re.IGNORECASE)
ADD_CONSTRAINT_RE = re.compile(r"\bADD\s+CONSTRAINT\b", re.IGNORECASE)
NOT_VALID_RE = re.compile(r"\bNOT\s+VALID\b", re.IGNORECASE)
DESTRUCTIVE_RE = re.compile(
    r"\b(DROP\s+(TABLE|COLUMN|TYPE|FUNCTION|TRIGGER|CONSTRAINT)|RENAME\b"
    r"|ALTER\s+COLUMN\s+\S+\s+(SET\s+DATA\s+)?TYPE|ALTER\s+COLUMN\s+\S+\s+SET\s+NOT\s+NULL)",
    re.IGNORECASE,
)
CREATED_RE = re.compile(
    r"^CREATE\s+(?:OR\s+REPLACE\s+)?(?:TABLE|TYPE|FUNCTION|TRIGGER)\s+(?:IF\s+NOT\s+EXISTS\s+)?([\w.]+)", re.IGNORECASE
)
DROPPED_RE = re.compile(r"^DROP\s+(?:TABLE|TYPE|FUNCTION|TRIGGER)\s+(?:IF\s+EXISTS\s+)?([\w.]+)", re.IGNORECASE)
DROP_CONSTRAINT_RE = re.compile(r"\bDROP\s+CONSTRAINT\s+(?:IF\s+EXISTS\s+)?(\w+)", re.IGNORECASE)
ADDED_CONSTRAINT_RE = re.compile(r"\bADD\s+CONSTRAINT\s+(\w+)", re.IGNORECASE)
CONTRACT_RE = re.compile(r"^--\s*contract:\s*unused since v(\d+)\.(\d+)\.(\d+)\b", re.IGNORECASE | re.MULTILINE)
VERSION_RE = re.compile(r"^v(\d+)\.(\d+)\.(\d+)")


def check_duplicates(names):
    errors, seen = [], {}
    for name in names:
        match = NAME_RE.match(name)
        if not match:
            errors.append(f"{name}: name must be NNNN_snake_case.sql")
            continue
        number = int(match.group(1))
        if number in seen:
            errors.append(f"{name}: number {number:04d} is already used by {seen[number]}")
        else:
            seen[number] = name
    return errors


def normalize(body):
    # Windows checkouts may turn LF into CRLF; that is not an edit.
    return body.replace(b"\r\n", b"\n")


def check_released(released, current):
    errors = []
    for name, body in sorted(released.items()):
        if name not in current:
            errors.append(f"{name}: released migration was deleted")
        elif normalize(current[name]) != normalize(body):
            errors.append(f"{name}: released migration was edited; add a new migration instead")
    return errors


def statements(sql):
    no_comments = "\n".join(line.split("--", 1)[0] for line in sql.splitlines())
    return [s.strip() for s in no_comments.split(";") if s.strip()]


def destructive_target(stmt):
    """The object a destructive statement changes: the table of an ALTER TABLE, else the dropped object."""
    match = ALTER_TABLE_RE.match(stmt) or DROPPED_RE.match(stmt)
    return match.group(1).lower() if match else None


def is_destructive(stmt, created, added):
    if destructive_target(stmt) in created:
        return False
    # A constraint dropped and added again by the same file (a widened CHECK) is replaced, not removed.
    rest = DROP_CONSTRAINT_RE.sub(lambda m: "" if m.group(1).lower() in added else m.group(0), stmt)
    return bool(DESTRUCTIVE_RE.search(rest))


def check_contract(name, sql, stmts, released):
    created = {m.group(1).lower() for s in stmts if (m := CREATED_RE.match(s))}
    added = {m.group(1).lower() for s in stmts for m in ADDED_CONSTRAINT_RE.finditer(s)}
    risky = [s for s in stmts if is_destructive(s, created, added)]
    if not risky:
        return []
    marks = [tuple(map(int, m.groups())) for m in CONTRACT_RE.finditer(sql)]
    if released is not None and any(mark <= released for mark in marks):
        return []
    first = " ".join(risky[0].split())[:80]
    last = "unknown (no v* tag)" if released is None else "v{}.{}.{}".format(*released)
    return [
        f"{name}: destructive statement ({first}) needs `-- contract: unused since vX.Y.Z` with X.Y.Z at or "
        f"below the last release ({last}); ship the code that stops using it first (expand/contract)"
    ]


def check_new_sql(name, sql, released=None):
    errors = []
    stmts = statements(sql)
    created = {m.group(1).lower() for s in stmts if (m := CREATE_TABLE_RE.match(s))}
    errors += check_contract(name, sql, stmts, released)
    for stmt in stmts:
        index = CREATE_INDEX_RE.match(stmt)
        if index:
            concurrent, table = bool(index.group(1)), index.group(2).lower()
            if concurrent:
                if not sql.startswith("-- no-transaction"):
                    errors.append(f"{name}: CREATE INDEX CONCURRENTLY needs `-- no-transaction` as the first line")
                if len(stmts) != 1:
                    errors.append(f"{name}: a CONCURRENTLY file must hold exactly one statement")
            elif table in BIG_TABLES and table not in created:
                errors.append(f"{name}: CREATE INDEX on big table {table} must use CONCURRENTLY")
        alter = ALTER_TABLE_RE.match(stmt)
        if alter and ADD_CONSTRAINT_RE.search(stmt) and not NOT_VALID_RE.search(stmt):
            table = alter.group(1).lower()
            if table in BIG_TABLES and table not in created:
                errors.append(f"{name}: ADD CONSTRAINT on big table {table} must be NOT VALID, then VALIDATE")
    return errors


def git(repo, *args):
    return subprocess.run(["git", "-C", repo, *args], capture_output=True, check=True).stdout


def resolve_base(repo, requested):
    if requested:
        return requested
    tags = git(repo, "tag", "--list", "v*", "--sort=-v:refname").decode().split()
    if tags:
        return tags[0]
    for ref in ("origin/master", "master"):
        probe = subprocess.run(["git", "-C", repo, "rev-parse", "--verify", "--quiet", ref], capture_output=True)
        if probe.returncode == 0:
            return ref
    return None


def released_version(repo, base):
    """(X, Y, Z) of the newest v* tag reachable from base, or None."""
    probe = subprocess.run(
        ["git", "-C", repo, "describe", "--tags", "--abbrev=0", "--match", "v*", base], capture_output=True
    )
    match = VERSION_RE.match(probe.stdout.decode().strip()) if probe.returncode == 0 else None
    return tuple(map(int, match.groups())) if match else None


def released_files(repo, base, folder):
    paths = git(repo, "ls-tree", "-r", "--name-only", base, "--", folder).decode().split("\n")
    files = {}
    for path in filter(None, paths):
        name = path.rsplit("/", 1)[-1]
        if NAME_RE.match(name):
            files[name] = git(repo, "show", f"{base}:{path}")
    return files


def current_files(repo, folder):
    root = pathlib.Path(repo) / folder
    return {p.name: p.read_bytes() for p in sorted(root.glob("*.sql"))}


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    parser.add_argument("--base", help="ref whose migrations count as released")
    parser.add_argument("--dir", default="backend/migrations", help="migrations folder, relative to the repo root")
    args = parser.parse_args(argv)

    repo = git(".", "rev-parse", "--show-toplevel").decode().strip()
    base = resolve_base(repo, args.base)
    if base is None:
        print("check-migrations: no base ref (fetch master or pass --base); refusing to pass unchecked")
        return 1

    current = current_files(repo, args.dir)
    released = released_files(repo, base, args.dir)
    errors = check_duplicates(current)
    errors += check_released(released, current)
    new = [name for name in current if name not in released]
    last = released_version(repo, base)
    for name in new:
        errors += check_new_sql(name, current[name].decode("utf-8"), last)

    for error in errors:
        print(f"check-migrations: {error}")
    if errors:
        return 1
    print(f"check-migrations: ok: {len(current)} migrations, {len(new)} new since {base}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
