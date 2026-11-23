#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Self-test for deploy/agent.sh (the root job runner). Runs in a sandbox: a fake `dahae` records its arguments,
# no Docker, no root. Usage: bash scripts/ci/agent-selftest.sh
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SB=$(mktemp -d)
trap 'rm -rf "$SB"' EXIT
PASS=0 FAIL=0

t()  { if "${@:2}"; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); echo "FAIL: $1"; fi; }
nt() { if "${@:2}"; then FAIL=$((FAIL + 1)); echo "FAIL (should fail): $1"; else PASS=$((PASS + 1)); fi; }
eq() { [ "$1" = "$2" ] || { echo "   got [$1] want [$2]"; return 1; }; }

# sandbox layout and a fake CLI that only records its arguments and DAHAE_YES
export DAHAE_JOBS="$SB/jobs" DAHAE_HOST_DIR="$SB/host" DAHAE_LOCK="$SB/lock" DAHAE_DIR="$SB"
export DAHAE_CLI="$SB/dahae"
mkdir -p "$DAHAE_JOBS/inbox" "$DAHAE_JOBS/work/done" "$DAHAE_JOBS/status" "$DAHAE_JOBS/public" "$DAHAE_JOBS/out" "$DAHAE_HOST_DIR"
# fake dahae: `update` or `domain` exits 3 (rolled back) once after `touch $SB/rollback-next`, or 1 after `touch $SB/fail-next`
cat > "$DAHAE_CLI" <<EOF
#!/usr/bin/env bash
printf '%s YES=%s\n' "\$*" "\${DAHAE_YES:-}" >> "$SB/argv.log"
# the next command exits with the code in \$SB/rc-next (once)
if [ -f "$SB/rc-next" ]; then rc=\$(cat "$SB/rc-next"); rm -f "$SB/rc-next"; exit "\$rc"; fi
# keys-backup: seals a fake copy with the passphrase it was handed; rollback exits 4 after touch \$SB/payouts-next
if [ "\$1" = keys-backup ]; then mkdir -p "$SB/backups"; printf 'sealed:%s' "\${DAHAE_KEYS_PASSPHRASE:-}" > "$SB/backups/keys-\$(date +%Y%m%d-%H%M%S%N).env.enc"; exit 0; fi
if [ "\$1" = rollback ] && [ -e "$SB/payouts-next" ]; then rm -f "$SB/payouts-next"; exit 4; fi
case "\$1" in update|domain) ;; *) exit 0 ;; esac
if [ -e "$SB/rollback-next" ]; then rm -f "$SB/rollback-next"; exit 3; fi
if [ -e "$SB/fail-next" ]; then rm -f "$SB/fail-next"; exit 1; fi
exit 0
EOF
printf 'DOMAIN=myshop.co.kr\nCHANNEL=stable\nVERSION=stable\n' > "$DAHAE_DIR/.env"
chmod +x "$DAHAE_CLI"
DAHAE_JOB_UID=$(id -u); DAHAE_OWNER_UID=$(id -u); export DAHAE_JOB_UID DAHAE_OWNER_UID
# fake docker: records the working folder and argv of every compose call; `logs` prints the file $SB/logs.txt
mkdir -p "$SB/bin"
cat > "$SB/bin/docker" <<EOF
#!/usr/bin/env bash
printf '%s %s\n' "\$PWD" "\$*" >> "$SB/docker.log"
case "\$*" in *" logs "*) cat "$SB/logs.txt" 2>/dev/null || true ;; "image inspect"*) cat "$SB/label.txt" 2>/dev/null || exit 1 ;; esac
EOF
chmod +x "$SB/bin/docker"
# fake public-IP lookup and DNS lookups: the server is 203.0.113.7, the domain has one A and one AAAA record
cat > "$SB/bin/curl" <<'EOF'
#!/usr/bin/env bash
case "$*" in *" -6 "*) echo 2001:db8::1 ;; *) echo 203.0.113.7 ;; esac
EOF
# a file named stray-aaaa next to this script adds an AAAA record that points elsewhere; a4-only gives the
# IPv4-mapped answer glibc returns for a domain with no AAAA record
cat > "$SB/bin/getent" <<'EOF'
#!/usr/bin/env bash
case "$1" in
  ahostsv4) echo "203.0.113.7  STREAM myshop.co.kr" ;;
  ahostsv6) if [ -e "$(dirname "$0")/a4-only" ]; then echo "::ffff:203.0.113.7  STREAM myshop.co.kr"
            elif [ -e "$(dirname "$0")/stray-aaaa" ]; then echo "2001:db8::99  STREAM myshop.co.kr"; else echo "2001:db8::1  STREAM myshop.co.kr"; fi ;;
esac
EOF
# fake timeout: records the limit it was given, then runs the command
cat > "$SB/bin/timeout" <<EOF
#!/usr/bin/env bash
printf '%s\n' "\$1" >> "$SB/timeout.log"; shift; exec "\$@"
EOF
chmod +x "$SB/bin/curl" "$SB/bin/getent" "$SB/bin/timeout"
export PATH="$SB/bin:$PATH"

# shellcheck source=deploy/agent.sh
DAHAE_SELFTEST=1 . "$ROOT/deploy/agent.sh"
set +e

uuid7() { # uuid7 [밀리초]: 시각이 든 UUIDv7 을 만들어요 (기본: 지금)
  local ms="${1:-$(($(date +%s) * 1000))}" hex rnd
  hex=$(printf '%012x' "$ms")
  rnd=$(od -An -N10 -tx1 /dev/urandom | tr -d ' \n')
  printf '%s-%s-7%s-%x%s-%s\n' "${hex:0:8}" "${hex:8:4}" "${rnd:0:3}" $((8 + RANDOM % 4)) "${rnd:3:3}" "${rnd:6:12}"
}
now_ms() { echo $(($(date +%s) * 1000)); }
submit() { printf '%b' "$2" > "$DAHAE_JOBS/inbox/$1.job"; intake_all; } # submit 번호 본문(\n 이스케이프 가능)
state() { sed -n 's/.*"state":"\([a-z_]*\)".*/\1/p' "$DAHAE_JOBS/status/$1.json" 2>/dev/null; }
argv_lines() { [ -f "$SB/argv.log" ] || { echo 0; return; }; wc -l < "$SB/argv.log" | tr -d ' '; }
argv_has() { grep -qxF "$1" "$SB/argv.log" 2>/dev/null; }
# refuse 번호 본문 : 아무것도 실행되지 않고 상태가 rejected 가 되면 성공(거부됨)
refuse() {
  local before; before=$(argv_lines)
  submit "$1" "$2"
  [ "$(argv_lines)" = "$before" ] && [ "$(state "$1")" = rejected ]
}

# ---- UUIDv7 time window ----
id=$(uuid7)
t  "fresh id passes"           id_fresh "$id"
nt "old id (16 min) refused"   id_fresh "$(uuid7 $(( $(now_ms) - 960000 )))"
nt "future id refused"         id_fresh "$(uuid7 $(( $(now_ms) + 60000 )))"
# The API names a job with the real millisecond and the folder watch runs the agent within that same second:
# an id late in the current second must pass (it was refused as "future" when the check rounded down to seconds).
t  "id later in this second passes"  id_fresh "$(uuid7 $(( $(now_ms) + 999 )))"
t  "id 1.5 s ahead passes (skew)"    id_fresh "$(uuid7 $(( $(now_ms) + 1500 )))"
nt "id 5 s ahead refused"            id_fresh "$(uuid7 $(( $(now_ms) + 5000 )))"
nt "v4 id refused"             id_fresh "$(printf '%s' "$id" | sed 's/^\(.\{14\}\)7/\14/')"  # 14번째 글자가 버전

# ---- happy path: the fixed argv, step lines only, state ok ----
id=$(uuid7)
submit "$id" "id=$id\nkind=backup-now\n"
t  "backup-now runs dahae backup"  argv_has "backup YES=1"
t  "job state ok"                  eq "$(state "$id")" ok
t  "status log holds step lines only" test -z "$(grep -vE '^(▶|✔|✘|!) ' "$DAHAE_JOBS/status/$id.log")"
t  "moved job file is removed"     test ! -e "$DAHAE_JOBS/work/$id.job"

id=$(uuid7); submit "$id" "id=$id\nkind=channel\narg=beta\n"
t  "channel beta"                  argv_has "channel beta YES=1"
id=$(uuid7); submit "$id" "id=$id\nkind=update-skip\n"
t  "update-skip writes marker"     test -e "$DAHAE_HOST_DIR/skip-next-update"
id=$(uuid7); submit "$id" "id=$id\nkind=status-refresh\n"
t  "status-refresh writes host.json" grep -q '"checked"' "$DAHAE_HOST_DIR/host.json"
id=$(uuid7); submit "$id" "id=$id\nkind=update\n"
t  "update runs dahae update"       argv_has "update YES=1"
id=$(uuid7); submit "$id" "id=$id\nkind=restart\narg=api\n"
t  "restart api runs compose argv" grep -q ' compose --env-file .env restart api$' "$SB/docker.log"
id=$(uuid7); submit "$id" "id=$id\nkind=restart\narg=all\n"
t  "restart all runs compose argv" grep -q ' compose --env-file .env restart$' "$SB/docker.log"

# ---- replay: the same id is never run twice, and the old ok state stays ----
before=$(argv_lines)
submit "$id" "id=$id\nkind=status-refresh\n"
t  "replayed id not re-run"        eq "$(argv_lines)" "$before"
t  "replay keeps old state"        eq "$(state "$id")" ok

# ---- refusals: nothing runs, state rejected ----
id=$(uuid7); t  "unknown key refused"              refuse "$id" "id=$id\nkind=backup-now\nextra=1\n"
id=$(uuid7); t  "unknown kind (restore) refused"   refuse "$id" "id=$id\nkind=restore\narg=x.sql.gz\n"
id=$(uuid7); t  "shell in channel arg refused"     refuse "$id" "id=$id\nkind=channel\narg=beta;rm\n"
id=$(uuid7); t  "backslash in value refused"       refuse "$id" "id=$id\nkind=restart\narg=api\\\\\n"
id=$(uuid7); t  "CR in value refused"              refuse "$id" "id=$id\r\nkind=backup-now\r\n"
id=$(uuid7); t  "duplicate key refused"            refuse "$id" "id=$id\nkind=backup-now\nkind=update\n"
id=$(uuid7); t  "value with = refused"             refuse "$id" "id=$id\nkind=channel\narg=a=b\n"
id=$(uuid7); t  "restart bad service refused"      refuse "$id" "id=$id\nkind=restart\narg=mysql\n"
id=$(uuid7); t  "job id not matching file name"    refuse "$id" "id=$(uuid7)\nkind=backup-now\n"
id=$(uuid7); t  "oversize job refused"             refuse "$id" "id=$id\nkind=backup-now\narg=$(printf 'x%.0s' $(seq 5000))\n"
old=$(uuid7 $(( $(now_ms) - 960000 )))
t  "stale id (16 min) refused"     refuse "$old" "id=$old\nkind=backup-now\n"
t  "unknown key: rejected state"   eq "$(state "$id")" rejected

# ---- ownership, links and names ----
WRONG=$((DAHAE_JOB_UID + 1))
JOB_UID=$WRONG
id=$(uuid7); t  "wrong owner uid refused"          refuse "$id" "id=$id\nkind=backup-now\n"
# shellcheck disable=SC2034 # read by the sourced deploy/agent.sh
JOB_UID=$(id -u)
id=$(uuid7)
printf 'id=%s\nkind=backup-now\n' "$id" > "$DAHAE_JOBS/inbox/not-a-job.txt"
before=$(argv_lines); intake_all
t  "file with a bad name is ignored" eq "$(argv_lines)" "$before"
if ln -s /etc/hostname "$SB/link.job" 2>/dev/null; then
  mv "$SB/link.job" "$DAHAE_JOBS/inbox/$id.job"
  before=$(argv_lines); intake_all
  t  "symlinked job refused"       eq "$(argv_lines)" "$before"
else
  echo "skip: symlink cannot be created here"
fi

# ---- the intake copy is what gets checked ----
src="$SB/copy-src.job"; cp=$(mktemp "$SB/copy.XXXXXX")
id=$(uuid7); printf 'id=%s\nkind=backup-now\n' "$id" > "$src"
t  "a good job is copied"          job_copy "$src" "$cp"
t  "the copy passes the checks"    intake_check "$cp" "$id"
printf 'id=%s\nkind=restore\n' "$id" > "$cp"
nt "a copy with a bad kind is refused" intake_check "$cp" "$id"

# a writer that still holds the moved file changes only the original, never the checked copy
held="$SB/held.job"; moved="$SB/held-moved.job"; cp=$(mktemp "$SB/copy.XXXXXX")
id=$(uuid7); printf 'id=%s\nkind=backup-now\n' "$id" > "$held"
exec 7>>"$held"; mv "$held" "$moved"
t  "held copy is taken"            job_copy "$moved" "$cp"
printf 'id=%s\nkind=restore\n' "$id" >&7; exec 7>&-
t  "copy unaffected by held handle" intake_check "$cp" "$id"
t  "copy still runs backup-now"    eq "$JOB_KIND" backup-now

# ---- status files: a planted link is never followed ----
victim="$SB/victim"; echo keep > "$victim"
id=$(uuid7)
if ln -s "$victim" "$DAHAE_JOBS/status/$id.log" 2>/dev/null; then
  before=$(argv_lines); submit "$id" "id=$id\nkind=backup-now\n"
  t  "planted log link not followed" eq "$(cat "$victim")" keep
  t  "job runs despite planted log"  eq "$(argv_lines)" "$((before + 1))"
  t  "state written past planted log" eq "$(state "$id")" ok
else echo "skip: symlink cannot be created here"; fi
victim2="$SB/victim2"; id=$(uuid7)
if ln -s "$victim2" "$DAHAE_JOBS/status/$id.json" 2>/dev/null; then
  submit "$id" "id=$id\nkind=restore\n"
  t  "dangling link not followed on reject" test ! -e "$victim2"
else echo "skip: symlink cannot be created here"; fi

# ---- root folders: owner and write bits are checked ----
t  "status dir owned by the root owner" dir_ok "$DAHAE_JOBS/status"
OWNER_UID=$((OWNER_UID + 1))
nt "status dir with another owner refused" dir_ok "$DAHAE_JOBS/status"
OWNER_UID=$((OWNER_UID - 1))
chmod 777 "$DAHAE_JOBS/status"
if [ "$(stat -c %a "$DAHAE_JOBS/status")" = 777 ]; then
  nt "group/other writable dir refused" dir_ok "$DAHAE_JOBS/status"
else echo "skip: mode bits cannot be set here"; fi
chmod 755 "$DAHAE_JOBS/status"
t  "all root folders pass" dirs_ok

# ---- a job left in work/ by an earlier run is swept, never run ----
id=$(uuid7); printf 'id=%s\nkind=backup-now\n' "$id" > "$DAHAE_JOBS/work/$id.job"
before=$(argv_lines); sweep_work
t  "stale work job swept"        test ! -e "$DAHAE_JOBS/work/$id.job"
t  "stale work job not run"      eq "$(argv_lines)" "$before"

# ---- schedule kind: only HH:MM on a weekday, or off ----
id=$(uuid7); submit "$id" "id=$id\nkind=schedule\narg=mon@04:00\n"
t  "schedule runs update-schedule" argv_has "update-schedule mon@04:00 YES=1"
id=$(uuid7); submit "$id" "id=$id\nkind=schedule\narg=off\n"
t  "schedule off accepted"         argv_has "update-schedule off YES=1"

# ---- outcomes of update: a reverted update is rolled_back, a failed one is failed ----
touch "$SB/rollback-next"
id=$(uuid7); submit "$id" "id=$id\nkind=update\n"
t  "reverted update ends rolled_back" eq "$(state "$id")" rolled_back
touch "$SB/fail-next"
id=$(uuid7); submit "$id" "id=$id\nkind=update\n"
t  "failed update ends failed"        eq "$(state "$id")" failed

# ---- public progress: the token names the file, it holds the state only, and it goes 10 minutes after the end ----
tok=0123456789abcdef0123456789abcdef
id=$(uuid7); submit "$id" "id=$id\nkind=backup-now\ntoken=$tok\n"
t  "progress file holds the state" grep -qx '{"state":"ok"}' "$DAHAE_JOBS/public/$tok.json"
old=ffffffffffffffffffffffffffffffff
printf '{"state":"ok"}\n' > "$DAHAE_JOBS/public/$old.json"; touch -d '20 minutes ago' "$DAHAE_JOBS/public/$old.json"
printf '{"state":"ok"}\n' > "$DAHAE_JOBS/public/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json"
prune_old
t  "old progress file removed"  test ! -e "$DAHAE_JOBS/public/$old.json"
t  "fresh progress file kept"   test -e "$DAHAE_JOBS/public/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.json"

# ---- a job that was running when the server stopped is failed on the next start ----
id=$(uuid7); printf '{"id":"%s","kind":"backup-now","state":"running"}\n' "$id" > "$DAHAE_JOBS/status/$id.json"
recover_interrupted
t  "interrupted job marked failed" eq "$(state "$id")" failed
t  "interrupted job logged"        grep -q '중단됨' "$DAHAE_JOBS/status/$id.log"

# ---- old step logs are pruned after 30 days ----
id=$(uuid7); printf '▶ old\n' > "$DAHAE_JOBS/status/$id.log"; touch -d '40 days ago' "$DAHAE_JOBS/status/$id.log"
prune_old
t  "40-day-old step log removed" test ! -e "$DAHAE_JOBS/status/$id.log"

# ---- host.json: channel, version, required CLI revision, next automatic update ----
host_refresh
t  "host.json has channel"        grep -q '"channel":"stable"' "$DAHAE_HOST_DIR/host.json"
t  "host.json has min_cli"        grep -q '"min_cli":2' "$DAHAE_HOST_DIR/host.json"
t  "host.json next update set"    grep -q '"next_update":"20' "$DAHAE_HOST_DIR/host.json"
echo off > "$DAHAE_DIR/update-schedule"; host_refresh
t  "host.json next update null when off" grep -q '"next_update":null' "$DAHAE_HOST_DIR/host.json"
# ---- fail2ban counts for host.json (failed ssh logins, current bans) ----
t  "f2b total failed"                 eq "$(printf '   |- Total failed:\t12\n   |- Currently banned:\t1\n' | f2b_count 'Total failed')" 12
t  "f2b currently banned"             eq "$(printf '   |- Total failed:\t12\n   |- Currently banned:\t1\n' | f2b_count 'Currently banned')" 1
t  "f2b missing line is 0"            eq "$(printf 'nothing\n' | f2b_count 'Total failed')" 0
t  "host.json has ssh counts"         grep -q '"ssh_failed":' "$DAHAE_HOST_DIR/host.json"
# ssh ban alert: sent when the ban count rises, not on repeats or decreases
F2B_OUT="$SB/f2b-status.txt"
printf '   |- Total failed:\t3\n   |- Currently banned:\t2\n' > "$F2B_OUT"
printf '#!/usr/bin/env bash\ncat "%s"\n' "$F2B_OUT" > "$SB/bin/fail2ban-client"; chmod +x "$SB/bin/fail2ban-client"
rm -f "$DAHAE_HOST_DIR/ssh-banned.last"; : > "$SB/argv.log"; host_refresh
t  "first ssh ban count alerts"          argv_has "alert ssh 2 YES=1"
: > "$SB/argv.log"; host_refresh
nt "same ssh ban count does not alert"   grep -q "alert ssh" "$SB/argv.log"
printf '   |- Total failed:\t9\n   |- Currently banned:\t4\n' > "$F2B_OUT"; : > "$SB/argv.log"; host_refresh
t  "rising ssh ban count alerts again"   argv_has "alert ssh 4 YES=1"
printf '   |- Total failed:\t9\n   |- Currently banned:\t1\n' > "$F2B_OUT"; : > "$SB/argv.log"; host_refresh
nt "falling ssh ban count does not alert" grep -q "alert ssh" "$SB/argv.log"
rm -f "$DAHAE_DIR/update-schedule"

# ---- version floor: the highest recorded version; no record means update-tag is refused; a stable release beats its own beta ----
VH="$HOST_DIR/version-history"
rm -f "$VH"
nt "no history: update-tag refused"         tag_newer v2.1.0
printf 'v2.1.0\n' > "$VH"
nt "beta of the installed version refused"  tag_newer v2.1.0-beta.1
printf 'v2.1.0-beta.1\n' > "$VH"
t  "stable after its own beta allowed"      tag_newer v2.1.0
t  "next beta after a beta allowed"         tag_newer v2.1.0-beta.2
printf 'v2.1.0-beta.2\n' > "$VH"
nt "older beta refused"                     tag_newer v2.1.0-beta.1
printf 'v2.9.0\n' > "$VH"
t  "v2.10.0 is newer than v2.9.0"           tag_newer v2.10.0
printf 'v2.2.0\n' > "$VH"                   # recorded by a manual update, not by update-tag
nt "tag installed outside update-tag holds" tag_newer v2.1.5
rm -f "$VH"

# ---- server approval (TOTP): update-tag and rollback wait in one slot; a code for that job runs it once ----
SECRET=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ # the RFC 6238 test key ("12345678901234567890")
TOK1=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa TOK2=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb
TOK3=cccccccccccccccccccccccccccccccc TOK4=dddddddddddddddddddddddddddddddd
TOK5=eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee
ap() { printf 'code=%s\n' "$2" > "$DAHAE_JOBS/inbox/$1.approve"; intake_all; } # ap 번호 코드 : 승인 파일을 넣고 실행기를 한 번 돌려요
t  "totp RFC 6238 at T=59"          eq "$(totp_at "$SECRET" 59)" 287082
t  "totp RFC 6238 at T=1111111109"  eq "$(totp_at "$SECRET" 1111111109)" 081804
t  "totp RFC 6238 at T=1234567890"  eq "$(totp_at "$SECRET" 1234567890)" 005924
nt "totp key never given as an argument" grep -q 'hexkey' "$ROOT/deploy/agent.sh"
printf 'DOMAIN=myshop.co.kr\nCHANNEL=stable\nVERSION=stable\n' > "$DAHAE_DIR/.env"
id=$(uuid7); id=$(uuid7); t  "no approval key: update-tag refused" refuse "$id" "id=$id\nkind=update-tag\narg=v2.1.0\ntoken=$TOK1\n"
printf '%s\n' "$SECRET" > "$DAHAE_HOST_DIR/approval.key"; chmod 600 "$DAHAE_HOST_DIR/approval.key"
printf 'v2.0.0\n' > "$DAHAE_HOST_DIR/version-history"
id=$(uuid7); t  "older version refused"                refuse "$id" "id=$id\nkind=update-tag\narg=v1.9.0\ntoken=$TOK1\n"
id=$(uuid7); t  "same version refused"                 refuse "$id" "id=$id\nkind=update-tag\narg=v2.0.0\ntoken=$TOK1\n"
id=$(uuid7); t  "beta suffix refused on stable"        refuse "$id" "id=$id\nkind=update-tag\narg=v2.1.0-beta.1\ntoken=$TOK1\n"
id=$(uuid7); t  "update-tag without token refused"     refuse "$id" "id=$id\nkind=update-tag\narg=v2.1.0\n"
id=$(uuid7); t  "rollback without token refused"       refuse "$id" "id=$id\nkind=rollback\n"
printf 'DOMAIN=myshop.co.kr\nCHANNEL=beta\nVERSION=stable\n' > "$DAHAE_DIR/.env"
t  "beta suffix accepted on beta"  tag_ok v2.1.0-beta.1
printf 'DOMAIN=myshop.co.kr\nCHANNEL=stable\nVERSION=stable\n' > "$DAHAE_DIR/.env"

# A: the job waits, a wrong code keeps it waiting, the right code runs it once
id=$(uuid7); before=$(argv_lines)
submit "$id" "id=$id\nkind=update-tag\narg=v2.1.0\ntoken=$TOK1\n"
t  "update-tag waits for approval"        eq "$(state "$id")" awaiting_approval
t  "approval slot holds the job"          grep -qxF "id=$id" "$HOST_DIR/awaiting.job"
t  "summary is written for the console"   grep -q '"summary":"업데이트: v2.1.0' "$DAHAE_JOBS/public/$TOK1.json"
t  "nothing ran while waiting"            eq "$(argv_lines)" "$before"
id2=$(uuid7); submit "$id2" "id=$id2\nkind=rollback\ntoken=$TOK2\n"
t  "second high-impact job refused while one waits" eq "$(state "$id2")" rejected
ap "$id" 000000
t  "wrong code keeps the slot"            test -e "$HOST_DIR/awaiting.job"
t  "wrong code runs nothing"              eq "$(argv_lines)" "$before"
good=$(totp_at "$SECRET" "$(date +%s)")
ap "$(uuid7)" "$good"
t  "code for a job not in the slot ignored" test -e "$HOST_DIR/awaiting.job"
ap "$id" "$good"
t  "right code runs update v2.1.0"        argv_has "update v2.1.0 YES=1"
t  "approved job ends ok"                 eq "$(state "$id")" ok
t  "slot is empty after approval"         test ! -e "$HOST_DIR/awaiting.job"
t  "version history records v2.1.0"       grep -qx v2.1.0 "$HOST_DIR/version-history"
nt "downgrade after approval refused"     tag_newer v2.0.0

# B: the same code is never accepted twice; five wrong codes cancel the job
printf 'DOMAIN=myshop.co.kr\nCHANNEL=stable\nVERSION=stable\nPREV_API_IMAGE=repo/api@sha256:aa\n' > "$DAHAE_DIR/.env"; printf '1.6.1\n' > "$SB/label.txt"
id2=$(uuid7); submit "$id2" "id=$id2\nkind=rollback\ntoken=$TOK2\n"
t  "rollback waits for approval"          eq "$(state "$id2")" awaiting_approval
t  "rollback summary names the version it goes back to" grep -q '"summary":"이전 버전으로 되돌려요: v1.6.1 ' "$DAHAE_JOBS/public/$TOK2.json"
# shellcheck disable=SC2016 # literal text on purpose
printf '$(id)\n' > "$SB/label.txt"
t  "an odd version label is never shown"  eq "$(prev_version)" '버전 모름'
rm -f "$SB/label.txt"; printf 'DOMAIN=myshop.co.kr\nCHANNEL=stable\nVERSION=stable\n' > "$DAHAE_DIR/.env"
before=$(argv_lines)
ap "$id2" "$good"
t  "reused code refused"                  eq "$(argv_lines)" "$before"
t  "reused code keeps the slot"           test -e "$HOST_DIR/awaiting.job"
s0=$(( $(date +%s) / 30 ))
for w in 000001 000002 000003 000004; do ap "$id2" "$w"; done
t  "five wrong codes cancel the job"      eq "$(state "$id2")" cancelled
t  "cancel burns every code still valid"  test "$(last_step)" -ge $((s0 + 1))
t  "cancel replaces the public summary"   grep -q '"state":"cancelled"' "$DAHAE_JOBS/public/$TOK2.json"
t  "cancel empties the slot"              test ! -e "$HOST_DIR/awaiting.job"

# C: an unanswered job expires after its time is up
id3=$(uuid7); submit "$id3" "id=$id3\nkind=rollback\ntoken=$TOK3\n"
printf 'expires=1\ntries=0\n' > "$HOST_DIR/awaiting.meta"
expire_slot
t  "expired job is cancelled"             eq "$(state "$id3")" cancelled
t  "expiry empties the slot"              test ! -e "$HOST_DIR/awaiting.job"

# D: a fresh code runs rollback; the fixed argv is the only thing that runs
printf '0\n' > "$HOST_DIR/approval.last-step"
id4=$(uuid7); submit "$id4" "id=$id4\nkind=rollback\ntoken=$TOK4\n"
good=$(totp_at "$SECRET" "$(date +%s)")
ap "$id4" "$good"
t  "approved rollback runs rollback"      argv_has "rollback YES=1"
t  "approved rollback ends ok"            eq "$(state "$id4")" ok
t  "rollback result shown publicly"      grep -q '"state":"ok"' "$DAHAE_JOBS/public/$TOK4.json"

# D2: one job per run, approvals first: the approved job runs and a queued job waits for the next run
printf '0\n' > "$HOST_DIR/approval.last-step"
idQ=$(uuid7); submit "$idQ" "id=$idQ\nkind=rollback\ntoken=$TOK5\n"
idR=$(uuid7); printf 'id=%s\nkind=backup-now\n' "$idR" > "$DAHAE_JOBS/inbox/$idR.job"
ap "$idQ" "$(totp_at "$SECRET" "$(date +%s)")"
t  "approval runs before a queued job"     eq "$(state "$idQ")" ok
t  "...and the queued job waits"           test -e "$DAHAE_JOBS/inbox/$idR.job"
intake_all
t  "the next run starts the queued job"    eq "$(state "$idR")" ok

# E: ten wrong codes within an hour lock approval: the right code is refused, a new high-impact job is refused
rm -f "$HOST_DIR/approval.fails"
idE=$(uuid7); submit "$idE" "id=$idE\nkind=rollback\ntoken=$TOK1\n"
for w in 000011 000012 000013 000014 000015; do ap "$idE" "$w"; done
idF=$(uuid7); submit "$idF" "id=$idF\nkind=rollback\ntoken=$TOK2\n"
for w in 000021 000022 000023 000024 000025; do ap "$idF" "$w"; done
t  "ten wrong codes are recorded"          eq "$(wc -l < "$HOST_DIR/approval.fails" | tr -d ' ')" 10
t  "ten wrong codes lock approval"         approval_locked
idG=$(uuid7); t  "new high-impact job refused while locked" refuse "$idG" "id=$idG\nkind=rollback\ntoken=$TOK3\n"
idH=$(uuid7); printf 'id=%s\nkind=rollback\ntoken=%s\n' "$idH" "$TOK4" > "$DAHAE_JOBS/inbox/$idH.job"
rm -f "$HOST_DIR/approval.fails"; intake_all
t  "waiting job held before the lock"      eq "$(state "$idH")" awaiting_approval
: > "$HOST_DIR/approval.fails"
for _ in 1 2 3 4 5 6 7 8 9 10; do date +%s >> "$HOST_DIR/approval.fails"; done
printf '0\n' > "$HOST_DIR/approval.last-step"       # the two cancels above burned the current codes
good=$(totp_at "$SECRET" $(( $(date +%s) + 30 )))   # the next 30-second step: not used yet
before=$(argv_lines)
ap "$idH" "$good"
t  "right code refused while locked"       eq "$(argv_lines)" "$before"
t  "waiting job stays while locked"        test -e "$HOST_DIR/awaiting.job"
: > "$HOST_DIR/approval.fails"
for _ in 1 2 3 4 5 6 7 8 9 10; do echo $(( $(date +%s) - 7200 )) >> "$HOST_DIR/approval.fails"; done
nt "old failures do not lock"              approval_locked
ap "$idH" "$good"
t  "right code runs once the lock lifts"   eq "$(state "$idH")" ok

# F: the enrolment job makes the key once, records the owner, and publishes the address to out/<token>
rm -f "$HOST_DIR/approval.key" "$HOST_DIR/owner.json"
STAFF=0190a6f2-1111-7000-8000-000000000001
idI=$(uuid7); submit "$idI" "id=$idI\nkind=host-approval-enroll\narg=$STAFF\ntoken=$TOK5\n"
t  "enroll job ends ok"                    eq "$(state "$idI")" ok
t  "enroll writes a 32-character key"      eq "$(wc -c < "$HOST_DIR/approval.key" | tr -d ' ')" 33
t  "enroll records the owner"              grep -qF "\"staff_id\":\"$STAFF\"" "$HOST_DIR/owner.json"
t  "enroll publishes the address"          grep -q 'otpauth://totp/myshop.co.kr:server?secret=' "$DAHAE_JOBS/out/$TOK5"
t  "the issuer is this server's domain"    grep -q '&issuer=myshop.co.kr&' "$DAHAE_JOBS/out/$TOK5"
idJ=$(uuid7); submit "$idJ" "id=$idJ\nkind=host-approval-enroll\narg=$STAFF\ntoken=$TOK5\n"
t  "second enroll fails"                   eq "$(state "$idJ")" failed
t  "...and the status says why"            grep -q '"step":"이미 서버 승인 코드가 있어요"' "$DAHAE_JOBS/status/$idJ.json"
id=$(uuid7); t  "enroll without a token refused" refuse "$id" "id=$id\nkind=host-approval-enroll\narg=$STAFF\n"
id=$(uuid7); t  "enroll with a bad owner id refused" refuse "$id" "id=$id\nkind=host-approval-enroll\narg=not-a-uuid\ntoken=$TOK5\n"
printf '%s\n' "$SECRET" > "$HOST_DIR/approval.key"

# G: domain-set waits for the server approval code, then runs `dahae domain`; a refusal says why in the step text
printf '0\n' > "$HOST_DIR/approval.last-step"; rm -f "$HOST_DIR/approval.fails"
idK=$(uuid7); before=$(argv_lines)
submit "$idK" "id=$idK\nkind=domain-set\narg=new.co.kr\ntoken=$TOK2\n"
t  "domain-set waits for approval"         eq "$(state "$idK")" awaiting_approval
t  "domain-set summary names the domain"   grep -q '"summary":"도메인을 new.co.kr 로 바꿔요' "$DAHAE_JOBS/public/$TOK2.json"
t  "nothing ran before approval"           eq "$(argv_lines)" "$before"
idL=$(uuid7); submit "$idL" "id=$idL\nkind=domain-set\narg=other.co.kr\ntoken=$TOK3\n"
t  "a second move is refused while one waits" eq "$(state "$idL")" rejected
t  "...and the status says why"            grep -q '"step":"다른 작업이 서버 승인을 기다리고 있어요"' "$DAHAE_JOBS/status/$idL.json"
t  "...and so does the step log"           grep -q '다른 작업이 서버 승인을 기다리고' "$DAHAE_JOBS/status/$idL.log"
ap "$idK" "$(totp_at "$SECRET" "$(date +%s)")"
t  "approved move runs dahae domain"       argv_has "domain new.co.kr YES=1"
t  "approved move ends ok"                 eq "$(state "$idK")" ok
nt "...with no reason"                     grep -q '"step"' "$DAHAE_JOBS/status/$idK.json"
# a move the CLI reverted (exit 3) or refused says why, in the domain's own words
printf '0\n' > "$HOST_DIR/approval.last-step"; touch "$SB/rollback-next"
idN=$(uuid7); submit "$idN" "id=$idN\nkind=domain-set\narg=new.co.kr\ntoken=$TOK2\n"; ap "$idN" "$(totp_at "$SECRET" "$(date +%s)")"
t  "reverted move ends rolled_back"        eq "$(state "$idN")" rolled_back
t  "...and says the new address did not answer" grep -q '"step":"새 주소가 5분 안에 응답하지 않아 예전 도메인으로 되돌렸어요' "$DAHAE_JOBS/status/$idN.json"
nt "...not the update's wording"           grep -q '이전 버전으로' "$DAHAE_JOBS/status/$idN.log"
printf '0\n' > "$HOST_DIR/approval.last-step"; touch "$SB/fail-next"
idO=$(uuid7); submit "$idO" "id=$idO\nkind=domain-set\narg=new.co.kr\ntoken=$TOK2\n"; ap "$idO" "$(totp_at "$SECRET" "$(date +%s)")"
t  "refused move ends failed"              eq "$(state "$idO")" failed
t  "...and points at the A record"         grep -q '"step":"도메인을 바꾸지 못했어요: 새 도메인 A 레코드가' "$DAHAE_JOBS/status/$idO.json"
t  "...and the status is still JSON"       python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "$DAHAE_JOBS/status/$idO.json"
id=$(uuid7); t  "domain-set without a token refused"  refuse "$id" "id=$id\nkind=domain-set\narg=new.co.kr\n"
id=$(uuid7); t  "domain-set to localhost refused"     refuse "$id" "id=$id\nkind=domain-set\narg=localhost\ntoken=$TOK3\n"
id=$(uuid7); t  "domain-set with shell chars refused" refuse "$id" "id=$id\nkind=domain-set\narg=a.com;id\ntoken=$TOK3\n"
mv "$HOST_DIR/approval.key" "$HOST_DIR/approval.key.off"
idM=$(uuid7); submit "$idM" "id=$idM\nkind=domain-set\narg=new.co.kr\ntoken=$TOK3\n"
t  "no approval key: domain-set refused"   eq "$(state "$idM")" rejected
t  "...with the reason in the status"      grep -q '"step":"서버 승인 코드가 없어요' "$DAHAE_JOBS/status/$idM.json"
t  "...and the status is still JSON"       python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "$DAHAE_JOBS/status/$idM.json"
mv "$HOST_DIR/approval.key.off" "$HOST_DIR/approval.key"

# ---- results written to out/<token>: logs are masked, read-only folders, 10 minutes ----
cat > "$SB/logs.txt" <<'LOG'
GET /admin Authorization: Bearer abc.def-ghi
session eyJhbGciOi.eyJzdWIiOiIxIn0.sig1234
DATA_KEY=supersecretvalue
db postgres://app:hunter2@db:5432/cleaning
{"token":"tokenvalue123","name":"kim"}
setup code ABCD-EFGH-JKMN
Authorization: Basic dXNlcjpwYXNzd29yZA==
Set-Cookie: sid=cookiesecret99; Path=/
toss test_sk_livesecret123 used
setup code ABCDEFGHJKMN
phone 010-1234-5678 and 01098765432
secret=alpha,beta
GET /health 200
LOG
id=$(uuid7); submit "$id" "id=$id\nkind=logs\narg=api:200\ntoken=$TOK5\n"
t  "logs result written to out/"          test -f "$DAHAE_JOBS/out/$TOK5"
t  "logs keep ordinary lines"             grep -q 'GET /health 200' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide the bearer token"           grep -q 'abc.def-ghi' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide the session token"          grep -q 'sig1234' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide the key value"              grep -q 'supersecretvalue' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide the database password"      grep -q 'hunter2' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide the json token"             grep -q 'tokenvalue123' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide the setup code"             grep -q 'ABCD-EFGH-JKMN' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide the basic credential"       grep -q 'dXNlcjpwYXNz' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide the cookie value"           grep -q 'cookiesecret99' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide the toss secret key"        grep -q 'livesecret123' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide a bare setup code"          grep -q 'ABCDEFGHJKMN' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide a hyphenated phone number"  grep -q '1234-5678' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide an unhyphenated phone"      grep -q '98765432' "$DAHAE_JOBS/out/$TOK5"
nt "logs hide the value after a comma"    grep -q 'beta' "$DAHAE_JOBS/out/$TOK5"
t  "logs run compose logs for api"        grep -q ' compose --env-file .env logs --no-color --tail 200 api$' "$SB/docker.log"
id=$(uuid7); submit "$id" "id=$id\nkind=logs\narg=api:10\ntoken=$TOK5\n"
t  "logs line count under 50 refused"     eq "$(state "$id")" rejected
touch -d '20 minutes ago' "$DAHAE_JOBS/out/$TOK5"; prune_old
t  "result file gone after 10 minutes"    test ! -e "$DAHAE_JOBS/out/$TOK5"

# domain-check: the server asks for its own IP; the answer is the A and AAAA records
id=$(uuid7); submit "$id" "id=$id\nkind=domain-check\narg=myshop.co.kr\ntoken=$TOK5\n"
t  "domain-check reports a match"         grep -q '"match":true' "$DAHAE_JOBS/out/$TOK5"
t  "domain-check reports the server IP"   grep -q '"server_ip":"203.0.113.7"' "$DAHAE_JOBS/out/$TOK5"
t  "domain-check lists the AAAA record"   grep -q '"aaaa":\["2001:db8::1"\]' "$DAHAE_JOBS/out/$TOK5"
t  "domain-check: AAAA is this server's"  grep -q '"aaaa_ok":true' "$DAHAE_JOBS/out/$TOK5"
touch "$SB/bin/stray-aaaa"
id=$(uuid7); submit "$id" "id=$id\nkind=domain-check\narg=myshop.co.kr\ntoken=$TOK5\n"
t  "stray AAAA elsewhere: aaaa_ok false"  grep -q '"aaaa_ok":false' "$DAHAE_JOBS/out/$TOK5"
t  "stray AAAA elsewhere: match false"    grep -q '"match":false' "$DAHAE_JOBS/out/$TOK5"
rm -f "$SB/bin/stray-aaaa"
touch "$SB/bin/a4-only"
id=$(uuid7); submit "$id" "id=$id\nkind=domain-check\narg=myshop.co.kr\ntoken=$TOK5\n"
t  "A record only: the mapped address is no AAAA" grep -q '"aaaa":\[\]' "$DAHAE_JOBS/out/$TOK5"
t  "A record only: match true"            grep -q '"match":true' "$DAHAE_JOBS/out/$TOK5"
rm -f "$SB/bin/a4-only"
id=$(uuid7); t  "domain-check takes a domain only" refuse "$id" "id=$id\nkind=domain-check\narg=localhost\ntoken=$TOK5\n"

# offsite-list, heartbeat
id=$(uuid7); submit "$id" "id=$id\nkind=offsite-list\ntoken=$TOK5\n"
t  "offsite-list runs restore-offsite"    argv_has "restore-offsite YES=1"
id=$(uuid7); submit "$id" "id=$id\nkind=heartbeat\narg=https://hc-ping.com/abc-123\n"
t  "heartbeat sets the ping address"      argv_has "heartbeat https://hc-ping.com/abc-123 YES=1"
id=$(uuid7); submit "$id" "id=$id\nkind=heartbeat\narg=off\n"
t  "heartbeat off"                        argv_has "heartbeat off YES=1"
id=$(uuid7); t  "heartbeat http refused"  refuse "$id" "id=$id\nkind=heartbeat\narg=http://hc-ping.com/x\n"
nt "heartbeat to loopback refused"        heartbeat_url_ok https://127.0.0.1:5432/
nt "heartbeat to metadata address refused" heartbeat_url_ok https://169.254.169.254/latest
nt "heartbeat to a private range refused" heartbeat_url_ok https://10.0.0.5/
nt "heartbeat to localhost refused"       heartbeat_url_ok https://localhost/
nt "heartbeat to an internal name refused" heartbeat_url_ok https://db/ping
t  "heartbeat to a public name accepted"  heartbeat_url_ok https://hc-ping.com/abc-123
id=$(uuid7); t  "heartbeat job to loopback refused" refuse "$id" "id=$id\nkind=heartbeat\narg=https://127.0.0.1:5432/\n"

# out/ is a root folder: a group-writable one is refused
chmod 777 "$DAHAE_JOBS/out"
if [ "$(stat -c %a "$DAHAE_JOBS/out")" = 777 ]; then nt "writable out/ refused" dirs_ok; else echo "skip: mode bits cannot be set here"; fi
chmod 755 "$DAHAE_JOBS/out"
expire_slot; clear_slot

# ---- shared fixtures (deploy/testdata/jobs/cases.txt): the same files the installer and the Rust tests read ----
FIX="$ROOT/deploy/testdata/jobs"
while read -r file want rest <&3; do
  case "$file" in ''|\#*) continue ;; esac
  id=$(uuid7); old=$(uuid7 $(( $(now_ms) - 960000 ))); fut=$(uuid7 $(( $(now_ms) + 60000 )))
  case "$(cat "$FIX/$file")" in *@OLD@*) name=$old ;; *@FUTURE@*) name=$fut ;; *) name=$id ;; esac
  sed -e "s/@ID@/$id/g" -e "s/@OLD@/$old/g" -e "s/@FUTURE@/$fut/g" "$FIX/$file" > "$DAHAE_JOBS/inbox/$name.job"
  before=$(argv_lines); intake_all
  if [ "$want" = refuse ]; then
    t  "fixture $file refused"  eq "$(argv_lines)" "$before"
    t  "fixture $file rejected" eq "$(state "$name")" rejected
  else
    t  "fixture $file runs: $rest" eq "$(tail -n1 "$SB/argv.log")" "$rest"
  fi
done 3< "$FIX/cases.txt"

# ---- one job per run: a second queued job waits in the inbox for the next run ----
a=$(uuid7); b=$(uuid7)
printf 'id=%s\nkind=backup-now\n' "$a" > "$DAHAE_JOBS/inbox/$a.job"
printf 'id=%s\nkind=schedule\narg=off\n' "$b" > "$DAHAE_JOBS/inbox/$b.job"
before=$(argv_lines); intake_all
t  "one run starts one job"               eq "$(argv_lines)" $((before + 1))
intake_all
t  "the next run starts the other"        eq "$(argv_lines)" $((before + 2))
t  "...and both end ok"                   eq "$(state "$a")$(state "$b")" okok

# ---- time limits: every job that backs up the photos first gets the long limit (last: update-tag writes history) ----
# shellcheck disable=SC2034 # JOB_ARG is read by dispatch in the sourced agent
limit_of() { JOB_KIND=$1; JOB_ARG=${2:-}; dispatch >/dev/null 2>&1; tail -n1 "$SB/timeout.log"; }
for k in update backup-now rollback; do t "$k gets the long limit" eq "$(limit_of "$k")" "$UPDATE_RUN_TIMEOUT"; done
t  "channel gets the long limit"          eq "$(limit_of channel beta)" "$UPDATE_RUN_TIMEOUT"
t  "update-tag gets the long limit"       eq "$(limit_of update-tag v9.9.9)" "$UPDATE_RUN_TIMEOUT"
t  "schedule keeps the normal limit"      eq "$(limit_of schedule off)" "$RUN_TIMEOUT"

# ---- keys backup from the console: the passphrase arrives as hex, the sealed copy goes out once as base64 ----
hexof() { printf '%s' "$1" | od -An -tx1 | tr -d ' \n'; }
KTOK=fedcba9876543210fedcba9876543210
id=$(uuid7); submit "$id" "id=$id\nkind=keys-backup\narg=$(hexof 'correct horse battery')\ntoken=$KTOK\n"
t  "keys-backup runs dahae keys-backup"      argv_has "keys-backup YES=1"
t  "keys-backup ends ok"                     eq "$(state "$id")" ok
t  "the CLI got the decoded passphrase"      eq "$(base64 -d "$DAHAE_JOBS/out/$KTOK" 2>/dev/null)" "sealed:correct horse battery"
t  "the output is one base64 line"           eq "$(wc -l < "$DAHAE_JOBS/out/$KTOK" | tr -d ' ')" 0
nt "the passphrase is not in the status"     grep -rqF "correct horse" "$DAHAE_JOBS/status"
KTOK2=0123456789abcdef0123456789abcde0
id=$(uuid7); submit "$id" "id=$id\nkind=keys-backup\narg=$(hexof '비밀번호한글열두자')\ntoken=$KTOK2\n"
t  "a Korean passphrase arrives byte for byte" eq "$(base64 -d "$DAHAE_JOBS/out/$KTOK2" 2>/dev/null)" "sealed:비밀번호한글열두자"
id=$(uuid7); t  "keys-backup without token refused"   refuse "$id" "id=$id\nkind=keys-backup\narg=$(hexof 'correct horse battery')\n"
id=$(uuid7); t  "keys-backup short passphrase refused" refuse "$id" "id=$id\nkind=keys-backup\narg=$(hexof 'elevenchars')\ntoken=$KTOK\n"
id=$(uuid7); t  "keys-backup upper-case hex refused"  refuse "$id" "id=$id\nkind=keys-backup\narg=$(hexof 'correct horse battery' | tr a-f A-F)\ntoken=$KTOK\n"
id=$(uuid7); t  "keys-backup odd hex refused"         refuse "$id" "id=$id\nkind=keys-backup\narg=$(hexof 'correct horse battery')a\ntoken=$KTOK\n"
before=$(argv_lines)
id=$(uuid7); submit "$id" "id=$id\nkind=keys-backup\narg=00$(hexof 'correct horse battery')\ntoken=$KTOK\n"
t  "a NUL byte in the passphrase fails the job" eq "$(state "$id")" failed
t  "...before the CLI runs"                  eq "$(argv_lines)" "$before"
id=$(uuid7 $(( $(now_ms) + 999 ))); submit "$id" "id=$id\nkind=keys-confirm\ntoken=$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')\n"
t  "keys-confirm runs dahae keys-confirm"    argv_has "keys-confirm YES=1"
t  "keys-confirm queued in the same second, with a token, ends ok" eq "$(state "$id")" ok
id=$(uuid7); t  "keys-confirm takes no argument"      refuse "$id" "id=$id\nkind=keys-confirm\narg=x\n"

# ---- rollback stopped by the payout guard says why ----
touch "$SB/payouts-next"
# shellcheck disable=SC2034 # read by run_job in the sourced agent
JOB_ID=$(uuid7) JOB_KIND=rollback JOB_ARG="" JOB_TOKEN=""; run_job
t  "payout guard: rollback ends failed"      eq "$(state "$JOB_ID")" failed
t  "payout guard: the reason names the payouts" grep -qF '정산' "$DAHAE_JOBS/status/$JOB_ID.json"

# ---- every way a rollback or an update can end has a reason the console shows ----
ends_with() { # ends_with 종류 종료코드 : 가짜 관리 명령이 그 코드로 끝날 때 상태 파일의 이유
  printf '%s\n' "$2" > "$SB/rc-next"
  # shellcheck disable=SC2034 # read by run_job in the sourced agent
  JOB_ID=$(uuid7) JOB_KIND=$1 JOB_ARG="" JOB_TOKEN=""; run_job
  printf '%s %s' "$(state "$JOB_ID")" "$(cat "$DAHAE_JOBS/status/$JOB_ID.json")"
}
# shellcheck disable=SC2016 # literal text on purpose
t  "rollback with no previous version says so" sh -c 'case "$1" in failed*"되돌릴 이전 버전이 없어요"*) ;; *) exit 1 ;; esac' _ "$(ends_with rollback 6)"
# shellcheck disable=SC2016 # literal text on purpose
t  "rollback whose tools stayed says to re-run the install line" sh -c 'case "$1" in failed*"관리 도구만 되돌리지 못했어요"*"C-5"*) ;; *) exit 1 ;; esac' _ "$(ends_with rollback 5)"
# shellcheck disable=SC2016 # literal text on purpose
t  "any other rollback failure points at the logs" sh -c 'case "$1" in failed*"dahae logs"*) ;; *) exit 1 ;; esac' _ "$(ends_with rollback 1)"
# shellcheck disable=SC2016 # literal text on purpose
t  "update whose tools stayed says the new version is fine" sh -c 'case "$1" in failed*"새 버전으로 바꿨고 정상이에요"*) ;; *) exit 1 ;; esac' _ "$(ends_with update 5)"

echo "agent self-test: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]
