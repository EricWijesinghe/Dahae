#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Self-test for deploy/install.sh and the `dahae` CLI it writes. No root, no Docker, nothing installed:
# sources both with DAHAE_SELFTEST=1 (main is skipped) and runs the pure functions in a sandbox dir.
# Usage: bash scripts/ci/installer-selftest.sh
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SB=$(mktemp -d)
trap 'rm -rf "$SB"' EXIT
PASS=0 FAIL=0

t()  { if "${@:2}"; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); echo "FAIL: $1"; fi; }    # t "what" cmd... : must succeed
nt() { if "${@:2}"; then FAIL=$((FAIL + 1)); echo "FAIL (should fail): $1"; else PASS=$((PASS + 1)); fi; }
eq() { [ "$1" = "$2" ] || { echo "   got [$1] want [$2]"; return 1; }; }

bash -n "$ROOT/deploy/install.sh"
bash -n "$ROOT/deploy/agent.sh"
# shellcheck source=deploy/install.sh
DAHAE_SELFTEST=1 . "$ROOT/deploy/install.sh"
set +e # the sourced installer turned on -e; failures are counted below, not fatal

# ---- installer: input checks ----
t  "domain plain"          valid_domain myshop.co.kr
t  "domain with scheme"    valid_domain https://MyShop.co.kr/admin/
t  "norm_domain"           eq "$(norm_domain 'https://MyShop.co.kr/x')" myshop.co.kr
nt "domain empty"          valid_domain ""
nt "domain no dot"         valid_domain localhost
nt "domain shell chars"    valid_domain 'a.com;rm -rf /'
nt "domain IP literal"     valid_domain 1.2.3.4
nt "domain double dot"     valid_domain a..b
nt "domain .local"         valid_domain x.local
nt "domain too long label" valid_domain "$(printf 'a%.0s' $(seq 64)).co.kr"
t  "domain punycode tld"   valid_domain shop.xn--3e0b707e
nt "email percent local"   valid_email 'a%b@shop.co.kr'
t  "email empty ok"        valid_email ""
t  "email ok"              valid_email owner@myshop.co.kr
nt "email bad"             valid_email 'owner@shop'
t  "heartbeat url"         valid_hb_url https://hc-ping.com/1f2e3d4c-aaaa-bbbb-cccc-123456789abc
nt "heartbeat http"        valid_hb_url http://hc-ping.com/x
nt "heartbeat ampersand"   valid_hb_url 'https://x.io/a?b=1&c=2'
# shellcheck disable=SC2016 # literal $(id) on purpose
nt "heartbeat subshell"    valid_hb_url 'https://x.io/$(id)'

# ---- installer: result ping (MALKKEUM_NOTIFY_URL, EXIT trap) ----
# ping_url <url> <exit code>: the address notify_result would call; curl is replaced by a recorder
ping_url() {
  rm -f "$SB/ping.out"
  ( curl() { printf '%s' "${*: -1}" > "$SB/ping.out"; }
    # shellcheck disable=SC2030 # the change is meant to stay in this subshell
    MALKKEUM_NOTIFY_URL="$1"; (exit "$2"); notify_result ) >/dev/null 2>&1
  cat "$SB/ping.out" 2>/dev/null || true
}
t  "notify success pings /0"        eq "$(ping_url https://hc-ping.com/abc 0)" https://hc-ping.com/abc/0
t  "notify failure pings /fail"     eq "$(ping_url https://hc-ping.com/abc 1)" https://hc-ping.com/abc/fail
t  "notify keeps the query"         eq "$(ping_url 'https://hc-ping.com/abc?x=1' 0)" 'https://hc-ping.com/abc/0?x=1'
t  "notify trailing slash"          eq "$(ping_url https://hc-ping.com/abc/ 0)" https://hc-ping.com/abc/0
t  "notify unset pings nothing"     eq "$(ping_url '' 0)" ""
t  "notify rejects & (no ping)"     eq "$(ping_url 'https://x.io/a&b' 0)" ""

# ---- install page (the browser side of cloud-init) ----
PAGE_DIR="$SB/page"
bash "$ROOT/scripts/release/build-install-page.sh" "$ROOT/deploy/install.sh" \
  "https://raw.githubusercontent.com/owner/repo/v1.0.0/install.sh" "$PAGE_DIR" >/dev/null
t  "install page pins install.sh sha256" grep -qF "$(sha256sum "$ROOT/deploy/install.sh" | cut -d' ' -f1)" "$PAGE_DIR/index.html"
nt "install page leaves no placeholder"  grep -q '__INSTALL_' "$PAGE_DIR/index.html"
# the browser makes setup codes from this alphabet; the installer must accept every one of them
t  "setup code alphabet: page = installer" eq \
  "$(sed -n 's/^const CODE_ALPHABET = "\([^"]*\)";/\1/p' "$PAGE_DIR/index.html")" \
  "$(sed -n 's/.*local alphabet=\([^ ]*\) .*/\1/p' "$ROOT/deploy/install.sh")"

# the page's three outputs: #cloud-config must parse as YAML, the #!/bin/bash and browser-SSH forms must pass bash -n.
# node runs the page's own generator functions (CI has node; in WSL set NODE_BIN to a node binary, e.g. node.exe)
NODE_BIN=${NODE_BIN:-$(command -v node || true)}
if [ -n "$NODE_BIN" ]; then
  page_outputs() { # page_outputs 파일 : 페이지 생성 함수를 실행해 세 가지 결과를 JSON 으로 남겨요
    python3 -I - "$PAGE_DIR/index.html" > "$SB/page-gen.js" <<'PY'
import json, re, sys
html = open(sys.argv[1], encoding="utf-8").read()
js = re.search(r"<script>(.*?)</script>", html, re.S).group(1)
core = js[:js.index("const $ = ")] + js[js.index("function cleanDomain"):js.index("function render()")]
p = {"domain": "myshop.co.kr", "email": "owner@example.kr", "notify": "https://hc-ping.com/abc", "code": "ABCD2345678J"}
print(core + "\nconst p = " + json.dumps(p) + ";\nprocess.stdout.write(JSON.stringify({cloud: buildCloudConfig(p), shell: buildShellScript(p), ssh: buildSshLine(p)}));")
PY
    "$NODE_BIN" - < "$SB/page-gen.js" > "$1"
  }
  page_part() { python3 -I -c 'import json,sys; sys.stdout.write(json.load(open(sys.argv[1]))[sys.argv[2]])' "$SB/page-out.json" "$1"; }
  cloud_ok() { # YAML parse when PyYAML is there; otherwise the structure the page promises
    python3 -I - "$1" <<'PY'
import sys
path = sys.argv[1]
text = open(path, encoding="utf-8").read()
try:
    import yaml
    doc = yaml.safe_load(text)
    cmd = doc["runcmd"][0]
    ok = doc.keys() == {"runcmd"} and cmd[0] == "bash" and cmd[1] == "-c" and "MALKKEUM_SETUP_CODE=ABCD2345678J" in cmd[2]
except ImportError:
    ok = text.startswith("#cloud-config\nruncmd:\n  - [bash, -c, \"") and "MALKKEUM_SETUP_CODE=ABCD2345678J" in text
sys.exit(0 if ok else 1)
PY
  }
  page_outputs "$SB/page-out.json"
  page_part cloud > "$SB/page-cloud.yml"; page_part shell > "$SB/page-shell.sh"; page_part ssh > "$SB/page-ssh.sh"
  t  "page: cloud-config parses"           cloud_ok "$SB/page-cloud.yml"
  t  "page: shell form starts with bash"   eq "$(head -n1 "$SB/page-shell.sh")" '#!/bin/bash'
  t  "page: shell form passes bash -n"     bash -n "$SB/page-shell.sh"
  t  "page: browser SSH line passes bash -n" bash -n "$SB/page-ssh.sh"
  t  "page: outputs carry the setup code"  sh -c "grep -qF MALKKEUM_SETUP_CODE=ABCD2345678J '$SB/page-cloud.yml' && grep -qF MALKKEUM_SETUP_CODE=ABCD2345678J '$SB/page-shell.sh' && grep -qF MALKKEUM_SETUP_CODE=ABCD2345678J '$SB/page-ssh.sh'"
else
  echo "note: install page outputs not checked (no node; set NODE_BIN)"
fi

# ---- installer: version compare ----
t  "ver_ge newer"          ver_ge 2.29.1 2.20
t  "ver_ge equal"          ver_ge 2.20 2.20
t  "ver_ge v prefix"       ver_ge v2.27.0 2.20
t  "ver_ge numeric order"  ver_ge 2.100.0 2.20
nt "ver_ge older"          ver_ge 2.17.3 2.20
nt "ver_ge v1"             ver_ge 1.29.2 2.20

# ---- installer: preflight parsing ----
cat > "$SB/meminfo" <<'EOF'
MemTotal:        1004532 kB
MemFree:          201220 kB
SwapTotal:             0 kB
EOF
t  "mem_kb MemTotal"       eq "$(mem_kb MemTotal "$SB/meminfo")" 1004532
t  "mem_kb SwapTotal"      eq "$(mem_kb SwapTotal "$SB/meminfo")" 0
t  "mem_kb missing key"    eq "$(mem_kb Nope "$SB/meminfo")" 0
t  "1 GB host needs swap"  test "$(mem_kb MemTotal "$SB/meminfo")" -lt "$SWAP_BELOW_KB"
t  "1 GB host allowed"     test "$(mem_kb MemTotal "$SB/meminfo")" -ge "$MIN_RAM_KB"
is_num() { case "$1" in ''|*[!0-9]*) return 1 ;; esac; }
t  "disk_free_kb number"   is_num "$(disk_free_kb "$SB")"
t  "arch x86_64"           arch_ok x86_64
t  "arch aarch64"          arch_ok aarch64
nt "arch armv7"            arch_ok armv7l
SS='LISTEN 0 511 0.0.0.0:80 0.0.0.0:* users:(("nginx",pid=812,fd=6),("nginx",pid=811,fd=6))
LISTEN 0 4096 0.0.0.0:443 0.0.0.0:* users:(("docker-proxy",pid=1200,fd=4))
LISTEN 0 511 [::]:443 [::]:* users:(("apache2",pid=900,fd=4))'
t  "ports: non-docker holders" eq "$(printf '%s\n' "$SS" | port_holders | tr '\n' ' ')" "apache2 nginx "
t  "ports: docker only"    eq "$(printf '%s\n' 'LISTEN 0 4096 0.0.0.0:80 0.0.0.0:* users:(("docker-proxy",pid=1,fd=4))' | port_holders)" ""
t  "ports: unknown holder" eq "$(printf '%s\n' 'LISTEN 0 4096 0.0.0.0:80 0.0.0.0:*' | port_holders)" "?"
t  "ports: none"           eq "$(printf '' | port_holders)" ""
t  "ipv4 filter (CNAME dropped)" eq "$(printf 'shop.example.net.\n203.0.113.7\n' | only_ipv4)" 203.0.113.7
t  "ip in list"            ip_in_list 203.0.113.7 "$(printf '198.51.100.1\n203.0.113.7')"
nt "ip not in list"        ip_in_list 203.0.113.70 "$(printf '203.0.113.7')"
nt "empty ip"              ip_in_list "" ""

# ---- the CLI the installer writes ----
DIR=$SB
write_cli "$SB/dahae"
write_heartbeat "$SB/heartbeat"
t  "CLI path substituted"  grep -q "^DIR=$SB\$" "$SB/dahae"
t  "CLI bash -n"           bash -n "$SB/dahae"
t  "heartbeat bash -n"     bash -n "$SB/heartbeat"
if command -v shellcheck >/dev/null; then
  t "CLI shellcheck"       shellcheck -S warning "$SB/dahae" "$SB/heartbeat"
else
  echo "skip: shellcheck not installed"
fi
cd "$SB"
printf 'DOMAIN=myshop.co.kr\nIMAGE_SOURCE=local\n' > .env
# shellcheck source=/dev/null
DAHAE_SELFTEST=1 . "$SB/dahae"
set +e
t  "uploads_for"           eq "$(uploads_for backups/db-20261110-033000.sql.gz)" backups/uploads-20261110-033000.tar.gz
t  "uploads_for sealed"    eq "$(uploads_for backups/db-20261110-033000.sql.gz.enc)" backups/uploads-20261110-033000.tar.gz
t  "fingerprint ok"        valid_fingerprints "$(printf 'AB:%.0s' $(seq 31))CD"
t  "two fingerprints"      valid_fingerprints "$(printf 'ab:%.0s' $(seq 31))cd,$(printf 'AB:%.0s' $(seq 31))EF"
nt "fingerprint short"     valid_fingerprints AB:CD
nt "fingerprint not hex"   valid_fingerprints "$(printf 'ZZ:%.0s' $(seq 31))CD"
t  "team id"               valid_team_id ABCDE12345
nt "team id short"         valid_team_id ABCDE
t  "env_get"               eq "$(env_get DOMAIN)" myshop.co.kr
env_set DOMAIN other.co.kr; env_set NEWKEY v=1
t  "env_set replace"       eq "$(env_get DOMAIN)" other.co.kr
t  "env_set add"           eq "$(env_get NEWKEY)" v=1
export YES=1 # read by confirm() in the sourced CLI
t  "confirm --yes"         confirm "q" yes
sub() { ( "$@" ) >/dev/null 2>&1; } # die exits: run it in a subshell
env_set ESC 'a\nB=c'                  # a literal backslash-n stays two characters on one line
t  "env_set keeps backslash-n"     eq "$(env_get ESC)" 'a\nB=c'
t  "env_set writes one line"       eq "$(grep -c '^B=' .env || true)" 0
nt "env_set refuses a newline"     sub env_set BAD "$(printf 'a\nB=c')"
nt "env_set refuses a CR"          sub env_set BAD "$(printf 'a\rB=c')"

# ---- restore-offsite only takes real backup names (no ../ paths) ----
t  "backup name"           valid_backup_name db-20261110-033000.sql.gz
t  "backup name .enc"      valid_backup_name db-20261110-033000.sql.gz.enc
nt "backup name traversal" valid_backup_name ../../etc/db-20261110-033000.sql.gz
nt "backup name dir"       valid_backup_name x/db-20261110-033000.sql.gz.enc
nt "backup name uploads"   valid_backup_name uploads-20261110-033000.tar.gz.enc
nt "backup name suffix"    valid_backup_name 'db-20261110-033000.sql.gz.enc;id'

# ---- uploads tar: checked first, then extracted beside the old files and swapped ----
mk_tar() { # mk_tar 이름 명령 : $SB/tsrc 에서 명령을 실행해 $SB/tar/<이름>.tar.gz 를 만들어요
  rm -rf "$SB/tsrc"; mkdir -p "$SB/tsrc/uploads/sub" "$SB/tar"
  (cd "$SB/tsrc" && sh -c "$2") || echo "mk_tar $1 failed"
}
export T="$SB/tar"
# shellcheck disable=SC2016 # $T and $PWD expand inside mk_tar's sh
{
  mk_tar good 'echo a > uploads/a.jpg; echo b > uploads/sub/b.jpg; tar czf "$T/good.tar.gz" uploads'
  mk_tar link 'ln -s /etc/passwd uploads/p; tar czf "$T/link.tar.gz" uploads'
  mk_tar hard 'echo a > uploads/a; ln uploads/a uploads/h; tar czf "$T/hard.tar.gz" uploads'
  mk_tar dotdot 'echo x > x; tar czPf "$T/dotdot.tar.gz" uploads uploads/../x'
  mk_tar abs 'echo x > uploads/a; tar czPf "$T/abs.tar.gz" uploads "$PWD/uploads/a"'
  mk_tar other 'mkdir etc; echo x > etc/x; tar czf "$T/other.tar.gz" uploads etc'
  mk_tar empty 'tar czf "$T/empty.tar.gz" -T /dev/null'
}
printf 'not a tar' | gzip > "$T/junk.tar.gz"
head -c 4096 /dev/urandom | gzip > "$T/noise.tar.gz"
nt "uploads tar empty"     uploads_tar_ok "$T/empty.tar.gz"
nt "uploads tar noise"     uploads_tar_ok "$T/noise.tar.gz"
t  "uploads tar ok"        uploads_tar_ok "$T/good.tar.gz"
nt "uploads tar symlink"   uploads_tar_ok "$T/link.tar.gz"
nt "uploads tar hardlink"  uploads_tar_ok "$T/hard.tar.gz"
nt "uploads tar .."        uploads_tar_ok "$T/dotdot.tar.gz"
nt "uploads tar absolute"  uploads_tar_ok "$T/abs.tar.gz"
nt "uploads tar outside uploads/" uploads_tar_ok "$T/other.tar.gz"
nt "uploads tar not a tar" uploads_tar_ok "$T/junk.tar.gz"
nt "uploads tar missing"   uploads_tar_ok "$T/none.tar.gz"
swap() { sh -c "${UPLOADS_SWAP//\/data\/uploads/$SB/vol}" < "$1" 2>/dev/null; } # the api container's script, on a sandbox dir
mkdir -p "$SB/vol"; echo old > "$SB/vol/old.jpg"
nt "swap: broken tar fails" swap "$T/noise.tar.gz"
t  "swap: old files kept when extract fails" test -f "$SB/vol/old.jpg"
t  "swap: no temp dir left after failure" test ! -e "$SB/vol/.restore-new"
t  "swap: good tar"        swap "$T/good.tar.gz"
t  "swap: new files in place" eq "$(cat "$SB/vol/a.jpg" "$SB/vol/sub/b.jpg" | tr '\n' ' ')" "a b "
t  "swap: old files gone"  test ! -e "$SB/vol/old.jpg"
t  "swap: temp dir gone"   test ! -e "$SB/vol/.restore-new"

# ---- uninstall --purge ignores --yes / DAHAE_YES: the domain must be typed or passed with --confirm ----
has_tty() { return 1; } # as under cron or CI: nobody to ask
nt "purge: --yes alone refused"  sub purge_ok ""
nt "purge: wrong domain refused" sub purge_ok wrong.co.kr
t  "purge: --confirm domain"     sub purge_ok other.co.kr
t  "plain uninstall still takes --yes" sub confirm "q" yes

# ---- image signatures: verified by digest, no silent skip ----
t  "signer ok"             valid_signer EricWijesinghe/Dahae
nt "signer regex chars"    valid_signer 'a/b|.*'
nt "signer no repo"        valid_signer EricWijesinghe
t  "signer identity regex" eq "$(signer_re Eric.W/mal.keum)" '(?i)^https://github\.com/Eric\.W/mal\.keum/\.github/workflows/release\.yml@refs/tags/v[0-9]'
printf 'abc' > "$SB/sum"
t  "sha256_is match"       sha256_is "$SB/sum" ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
nt "sha256_is mismatch"    sha256_is "$SB/sum" 0000000000000000000000000000000000000000000000000000000000000000
t  "cosign sha amd64"      eq "$(cosign_sha256 x86_64)" "$COSIGN_SHA256_AMD64"
t  "cosign sha arm64"      eq "$(cosign_sha256 aarch64)" "$COSIGN_SHA256_ARM64"
nt "cosign sha other arch" cosign_sha256 armv7l
mkdir -p "$SB/bin"
cat > "$SB/bin/cosign" <<'COS'
#!/usr/bin/env bash
printf '%s\n' "$*" > "$(dirname "$0")/args"
[ "${FAKE_COSIGN:-}" = ok ] || { echo "Error: no matching signatures" >&2; exit 1; }
echo 'Verification for x --' >&2
echo '[{"critical":{"image":{"docker-manifest-digest":"sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}}}]'
COS
chmod 755 "$SB/bin/cosign"
env_set SIGNER_REPO EricWijesinghe/Dahae
export FAKE_COSIGN=ok
t  "verify -> pinned digest" eq "$(verify_image ghcr.io/o/malkkeum-api:stable)" ghcr.io/o/malkkeum-api@sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
t  "verify pins the OIDC issuer" grep -q -- "--certificate-oidc-issuer https://token.actions.githubusercontent.com" "$SB/bin/args"
t  "verify pins release.yml on tags" grep -qF 'EricWijesinghe/Dahae/\.github/workflows/release\.yml@refs/tags/v' "$SB/bin/args"
FAKE_COSIGN=bad
nt "unsigned image refused" sub verify_image ghcr.io/o/malkkeum-api:stable
FAKE_COSIGN=ok; env_set SIGNER_REPO 'bad repo'
nt "bad signer refused"    sub verify_image ghcr.io/o/malkkeum-api:stable
env_set SIGNER_REPO EricWijesinghe/Dahae; rm -f "$SB/bin/cosign"
nt "missing cosign refused (no silent skip)" sub verify_image ghcr.io/o/malkkeum-api:stable
# release.yml publishes the image tag without the v (1.5.0); an update given v1.5.0 must pull that tag
pull_tag() { ( verify_image() { echo "$1"; }; docker() { return 0; }; env_set IMAGE_SOURCE remote; env_set IMAGE_VERIFY on; pull_images "$1" "$2"; echo "$NEW_API" ); }
t  "pull: v-form tag pulls the bare image tag" eq "$(pull_tag ghcr.io/o v1.5.0)" ghcr.io/o/malkkeum-api:1.5.0
t  "pull: bare tag unchanged"                  eq "$(pull_tag ghcr.io/o 1.5.0)" ghcr.io/o/malkkeum-api:1.5.0
t  "pull: channel name unchanged"              eq "$(pull_tag ghcr.io/o stable)" ghcr.io/o/malkkeum-api:stable
t  "pull: unsigned-test registry gets the bare tag too" eq "$( ( env_set IMAGE_VERIFY off; docker() { return 0; }; pull_images ghcr.io/o v1.5.0; echo "$NEW_API" ) | tail -n1)" ghcr.io/o/malkkeum-api:1.5.0
t  "pull: local tar keeps the tag it was loaded with" eq "$( ( env_set IMAGE_SOURCE local; docker() { return 0; }; pull_images ghcr.io/o v1.5.0; echo "$NEW_API" ) | tail -n1)" ghcr.io/o/malkkeum-api:v1.5.0
t  "pull: local tar loaded with the bare tag is found for v1.5.0" eq "$( ( env_set IMAGE_SOURCE local; docker() { case "$*" in *:1.5.0*) return 0 ;; *) return 1 ;; esac; }; pull_images ghcr.io/o v1.5.0; echo "$NEW_API" ) | tail -n1)" ghcr.io/o/malkkeum-api:1.5.0
env_set IMAGE_SOURCE remote; env_set IMAGE_VERIFY on

# ---- app DB role: not a superuser; restores drop owner/extension lines ----
SQL=$(db_roles_sql 0123456789abcdef0123456789abcdef)
t  "role is not superuser" grep -q 'NOSUPERUSER NOCREATEDB NOCREATEROLE' <<< "$SQL"
t  "role password set"     grep -q "PASSWORD '0123456789abcdef0123456789abcdef'" <<< "$SQL"
nt "no placeholders left"  grep -q '__ROLE__\|__PW__' <<< "$SQL"
t  "extensions made by the superuser" grep -q 'CREATE EXTENSION IF NOT EXISTS pg_trgm' <<< "$SQL"
DUMP="ALTER TABLE public.x OWNER TO app;
DROP EXTENSION IF EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;
COMMENT ON EXTENSION pgcrypto IS 'x';
CREATE TABLE public.x (a int);"
t  "restore filter drops owner and extension lines" eq "$(restore_filter <<< "$DUMP")" 'CREATE TABLE public.x (a int);'

# digest_ref with a fake docker: loaded (tar) images have no RepoDigests, so they get a keep- tag
ID=sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef
# shellcheck disable=SC2317 # called indirectly by digest_ref
docker() {
  case "$*" in
    "image inspect -f {{.Id}} "*) [ "$5" = gone:tag ] && return 1; echo "$ID" ;;
    "image inspect -f {{index .RepoDigests 0}} "*) [ -n "${FAKE_DIGEST:-}" ] && echo "$FAKE_DIGEST" || return 1 ;;
    "tag "*) echo "$2 $3" >> "$SB/tags" ;;
    *) echo "unexpected docker $*" >&2; return 1 ;;
  esac
}
t  "digest_ref local -> keep tag" eq "$(digest_ref r/malkkeum-api:local r/malkkeum-api)" r/malkkeum-api:keep-0123456789ab
t  "keep tag created"      grep -qx "$ID r/malkkeum-api:keep-0123456789ab" "$SB/tags"
env_set IMAGE_SOURCE remote
FAKE_DIGEST=r/malkkeum-api@sha256:feed
t  "digest_ref remote -> keep tag too (untagged images get pruned)" eq "$(digest_ref r/malkkeum-api:stable r/malkkeum-api)" r/malkkeum-api:keep-0123456789ab
FAKE_DIGEST=
t  "digest_ref unknown image -> as given" eq "$(digest_ref gone:tag r/x)" gone:tag
unset -f docker

# ---- .env is set to root-only on every installer run, even if it was loosened ----
chmod 644 .env
lock_env
t  ".env locked to 600"    eq "$(stat -c %a .env)" 600

# ---- the compose file install.sh writes = deploy/docker-compose.yml (minus comments, build and app image lines) ----
norm_compose() { sed -E -e '/^[[:space:]]*#/d' -e 's/[[:space:]]+#[[:space:]].*$//' -e '/^[[:space:]]*build:/d' \
  -e '/^[[:space:]]*image: \$\{(API|WEB)_IMAGE/d' -e '/^[[:space:]]*$/d' "$1"; }
mkdir -p "$SB/compose" && (cd "$SB/compose" && write_compose)
t  "embedded compose = deploy/docker-compose.yml" diff <(norm_compose "$ROOT/deploy/docker-compose.yml") <(norm_compose "$SB/compose/docker-compose.yml")
# shellcheck disable=SC2016 # literal ${...} on purpose
t  "embedded compose pins images from .env" eq "$(grep -c 'image: ${\(API\|WEB\)_IMAGE:?set by install.sh}' "$SB/compose/docker-compose.yml")" 2
t  "every service hardened" eq "$(grep -c '<<: \*hardened' "$ROOT/deploy/docker-compose.yml")" 4
for k in 'no-new-privileges:true' 'cap_drop: \[ALL\]' 'read_only: true'; do
  t "hardening has $k" grep -q "^  .*$k" "$ROOT/deploy/docker-compose.yml"
done
t  "every service memory-capped" eq "$(grep -c '^    mem_limit:' "$ROOT/deploy/docker-compose.yml")" 4
# one open file per app socket (two in Caddy): api and web lift Docker's default 1024 open files
for f in "$ROOT/deploy/docker-compose.yml" "$SB/compose/docker-compose.yml"; do
  t "api and web raise open files (${f#"$ROOT/"})" eq "$(grep -c '^    ulimits: { nofile: { soft: 65536, hard: 65536 } }$' "$f")" 2
done
t  "api ulimit sits in the api service" eq "$(awk '/^  [a-z-]+:$/ { s = $1 } /nofile/ { print s }' "$ROOT/deploy/docker-compose.yml" | tr '
' ' ')" "api: web: "
t  "web (Caddy) memory cap for long-lived sockets" grep -qx '    mem_limit: 384m' "$ROOT/deploy/docker-compose.yml"
# the sizing comment states the sum of the caps: keep it true when a cap changes
mem_sum() { awk '/^  [a-z-]+:$/ { s = $1 } /^    mem_limit:/ { v = $2; sub(/m$/, "", v); t += v; if (s != "uptime-kuma:") d += v } END { print d "m " t "m" }' "$1"; }
t  "mem caps add up to the sizing comment" eq "$(mem_sum "$ROOT/deploy/docker-compose.yml")" "1664m 1984m"
t  "sizing comment names the default total" grep -qF '= 1664m (1984m with the optional' "$ROOT/deploy/docker-compose.yml"
t  "default stack fits a 2 GB VPS with room for the host" test "$(mem_sum "$ROOT/deploy/docker-compose.yml" | cut -dm -f1)" -le 1700
nt "compose written in place (no leftover temp file)" test -e "$SB/compose/docker-compose.yml.new"

# ---- local backups: plaintext stays until its sealed copy exists; restore takes only backups/ paths ----
mkdir -p backups
api_backup() { return 1; } # the seal step fails
printf 'dump' > backups/db-t1.sql.gz
seal_local backups/db-t1.sql.gz >/dev/null 2>&1 || true
t  "seal failure keeps plaintext"     test -f backups/db-t1.sql.gz
nt "seal failure leaves no .enc"      test -e backups/db-t1.sql.gz.enc
api_backup() { [ -f "${2#/}" ] && printf 'MKB1' > "${3#/}"; } # the container's seal step, on this sandbox: needs its input
printf 'dump' > backups/db-t2.sql.gz
t  "seal success"                     seal_local backups/db-t2.sql.gz
nt "seal success removes plaintext"   test -e backups/db-t2.sql.gz
t  "seal success keeps .enc"          test -s backups/db-t2.sql.gz.enc
printf 'old' > backups/db-t3.sql.gz
seal_leftover_plaintext >/dev/null 2>&1
t  "leftover plaintext gets sealed"   test -s backups/db-t3.sql.gz.enc
t  "sealing an already sealed dump"   seal_local backups/db-t3.sql.gz
t  "...keeps its sealed copy"         test -s backups/db-t3.sql.gz.enc
t  "backup path in backups/"          backup_path_ok backups/db-t2.sql.gz.enc
nt "backup path outside backups/"     backup_path_ok /etc/passwd
nt "backup path bare name"            backup_path_ok db-t2.sql.gz.enc
nt "backup path traversal"            backup_path_ok backups/../x
t  "full path from 'dahae backup' maps into backups/" eq "$(backup_rel "$DIR/backups/db-t2.sql.gz.enc")" backups/db-t2.sql.gz.enc
t  "relative path unchanged"          eq "$(backup_rel backups/db-t2.sql.gz.enc)" backups/db-t2.sql.gz.enc
nt "other absolute path still refused" backup_path_ok "$(backup_rel /etc/passwd)"
unset -f api_backup
YES=1 key_escrow_confirm >/dev/null 2>&1
t  "--yes never attests key copy"     eq "$(env_get KEY_ESCROW_AT)" ""

# ---- secret key copy (dahae keys-backup / keys-restore) ----
printf 'DB_PASSWORD=%s\nDATA_KEY=%s\nJWT_SECRET=%s\n' "$(printf 'a%.0s' $(seq 48))" "$(printf 'b%.0s' $(seq 64))" "$(printf 'c%.0s' $(seq 64))" > "$SB/keys.env"
keys_seal 'correct horse battery' < "$SB/keys.env" > "$SB/keys.enc"
t  "sealed copy is not plain text" sh -c "! grep -q DATA_KEY '$SB/keys.enc'"
t  "sealed copy opens with its passphrase" eq "$(keys_open 'correct horse battery' < "$SB/keys.enc")" "$(cat "$SB/keys.env")"
opens() { keys_open "$1" < "$SB/keys.enc" >/dev/null; } # a wrong passphrase can print bytes before it fails
nt "wrong passphrase refused" opens 'wrong horse battery'
t  "DATA_KEY read back" eq "$(keys_field DATA_KEY < "$SB/keys.env")" "$(printf 'b%.0s' $(seq 64))"
# shellcheck disable=SC2016 # literal $(id) on purpose
t  "non-hex value ignored" eq "$(printf 'DATA_KEY=$(id)\n' | keys_field DATA_KEY)" ""

# ---- failed unattended jobs alert through the heartbeat URL ----
t  "fail url"              eq "$(fail_url https://hc-ping.com/abc)" https://hc-ping.com/abc/fail
t  "fail url, trailing /"  eq "$(fail_url https://hc-ping.com/abc/)" https://hc-ping.com/abc/fail
t  "fail url keeps query"  eq "$(fail_url 'https://x.io/p?k=1')" 'https://x.io/p/fail?k=1'
nt "alert takes update|backup only" sub cmd_alert ../../etc
for job in update backup; do
  cmd=$job; [ "$job" = update ] && cmd=auto-update
  t "cron alerts on a failed $job" grep -qF "dahae $cmd >> \$DIR/backups/$job.log 2>&1 || \$BIN/dahae alert $job" "$ROOT/deploy/install.sh"
done

# ---- publish-delivery.sh: channel branches + immutable annotated tag (a local bare repo stands in for DIST_REPO) ----
DIST=example-owner/example-install # any repo: the guides carry a URL token, not an account name
git init -q --bare "$SB/dist.git"
pub() { # pub 태그 : the release job's delivery step, pushing to $SB/dist.git instead of GitHub
  GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0="url.$SB/dist.git.insteadOf" GIT_CONFIG_VALUE_0="git@github.com:$DIST.git" \
    DIST_REPO=$DIST VER="$1" IMAGE_REPO=ghcr.io/o SIGNER_REPO=o/src bash "$ROOT/deploy/publish-delivery.sh" >/dev/null 2>&1
}
g() { git -C "$SB/dist.git" "$@"; }
has() { grep -qF -- "$2" <<< "$(g show "$1")"; } # has 태그:파일 문자열 (no pipe: grep -q + pipefail = SIGPIPE)
sums_ok() { [ "$(g show "$1:install.sh" | sha256sum | cut -d' ' -f1)" = "$(g show "$1:SHA256SUMS" | awk '{ sub(/^\*/, "", $2) } $2 == "install.sh" { print $1 }')" ]; }
t  "publish stable"        pub v1.0.0
t  "tag is annotated"      eq "$(g cat-file -t v1.0.0)" tag
t  "tag = main commit"     eq "$(g rev-parse 'v1.0.0^{commit}')" "$(g rev-parse main)"
t  "tag installer is stable" has v1.0.0:install.sh DEFAULT_CHANNEL=stable
t  "tag installer names the signer" has v1.0.0:install.sh SIGNER_REPO_DEFAULT=o/src
t  "tag SHA256SUMS matches" sums_ok v1.0.0
t  "tag guide uses the tag URL" has "v1.0.0:guide/01-서버-설치.md" "raw.githubusercontent.com/$DIST/v1.0.0/install.sh"
t  "tag README uses the tag URL" has v1.0.0:README.md "raw.githubusercontent.com/$DIST/v1.0.0/SHA256SUMS"
t  "beta guide follows the beta channel"  has "beta:guide/01-서버-설치.md" "raw.githubusercontent.com/$DIST/beta/install.sh"
nt "beta guide 00 names no page file"     has "beta:guide/00-한번에-설치.md" "dahae-install-page.html"
t  "tag guide 00 names the page file"    has "v1.0.0:guide/00-한번에-설치.md" "dahae-install-page.html"
# the install page is built for the tagged branch only: its hash must be the tag's install.sh
t  "tag ships the install page"          has v1.0.0:dahae-install-page.html "INSTALL_SH_SHA256 = \"$(g show v1.0.0:install.sh | sha256sum | cut -d' ' -f1)\""
nt "beta branch has no install page"     g cat-file -e "beta:dahae-install-page.html"
nt "beta README has no page link"        has beta:README.md "dahae-install-page.html"
t  "tag README has the page link"        has v1.0.0:README.md "dahae-install-page.html"
t  "tag guide 08 uses the tag URL"       has "v1.0.0:guide/08-클라우드-배포.md" "raw.githubusercontent.com/$DIST/v1.0.0/install.sh"
t  "first-install guide gets the URL too" has "v1.0.0:guide/00-한번에-설치.md" "raw.githubusercontent.com/$DIST/v1.0.0/install.sh"
nt "no URL token left in published guides" sh -c "git -C '$SB/dist.git' grep -q INSTALL_URL v1.0.0 -- guide"
nt "developer-only guide not shipped" sub g cat-file -e v1.0.0:guide/05-demo-install.md
nt "no link to the developer-only guide" sh -c "git -C '$SB/dist.git' grep -q 05-demo-install v1.0.0 -- guide"
t  "re-run is a no-op"     pub v1.0.0
t  "publish beta"          pub v1.1.0-beta.1
t  "beta tag = beta commit" eq "$(g rev-parse 'v1.1.0-beta.1^{commit}')" "$(g rev-parse beta)"
t  "beta tag installer is beta" has v1.1.0-beta.1:install.sh DEFAULT_CHANNEL=beta
t  "beta tag SHA256SUMS matches" sums_ok v1.1.0-beta.1
t  "stable tag unchanged by beta" eq "$(g rev-parse 'v1.0.0^{commit}')" "$(g rev-parse main)"
g -c user.name=t -c user.email=t@t tag -f -a v1.0.0 -m moved beta >/dev/null
nt "moved tag is refused"  pub v1.0.0

# ---- scheduled jobs: cron table, update gates, setup code expiry (a copy of the CLI writes into the sandbox, never /etc) ----
set +e
cd "$SB"
sed -e "s|/etc/cron.d/malkkeum|$SB/cron-out|" -e "s|__DIR__|$SB|" "$SB/dahae" > "$SB/dahae-cron"
cron_cli() { ( DAHAE_SELFTEST=1 . "$SB/dahae-cron"; cd "$SB"; "$@" ) >/dev/null 2>&1; }
has_updated() { case "$1" in *UPDATED*) return 0 ;; *) return 1 ;; esac; }
auto_run() { ( DAHAE_SELFTEST=1 . "$SB/dahae-cron"; cd "$SB"; cmd_update() { echo UPDATED; }; curl() { printf '%s
' "$*" >> "$SB/curl-auto.log"; [ -n "${CHAN_VER:-}" ] && printf '%s
' "$CHAN_VER"; }; docker() { case "$*" in *malkkeum-web*) [ -z "${WEB_FAIL:-}" ] || return 1 ;; esac; printf '%s' "$FAKE_MANIFEST"; }; cmd_auto_update ) 2>&1; }
setup_out() { ( DAHAE_SELFTEST=1 . "$SB/dahae-cron"; cd "$SB"; dc() { return 0; }; cmd_setup_code "$@" ) 2>/dev/null; }
rm -f "$SB/update-schedule"
cron_cli cmd_write_cron
t  "default cron: backup under the lock"  grep -qF "30 3 * * * root flock -w 3600 /run/dahae.lock /usr/local/bin/dahae backup" "$SB/cron-out"
t  "default cron: weekly update Monday"   grep -qF "0 4 * * 1 root flock -n -E 0 /run/dahae.lock /usr/local/bin/dahae auto-update" "$SB/cron-out"
t  "default cron: heartbeat kept"         grep -qF "*/5 * * * * root /usr/local/bin/malkkeum-heartbeat" "$SB/cron-out"
cron_cli cmd_update_schedule tue@06:30
t  "schedule tue 06:30 in the cron table" grep -qF "30 6 * * 2 root flock -n -E 0" "$SB/cron-out"
t  "schedule kept in its file"           eq "$(cat "$SB/update-schedule")" tue@06:30
nt "bad schedule refused"                cron_cli cmd_update_schedule mon@4:00
cron_cli cmd_update_schedule off
nt "off removes the update line"         grep -qF "auto-update" "$SB/cron-out"
t  "off keeps the backup line"           grep -qF "dahae backup" "$SB/cron-out"

# ---- WAL archiving: compose calls add the pgBackRest override once archiving is on; weekly base backup in cron ----
WALON="$SB/walon"; WALOFF="$SB/waloff"; NOWAL="$SB/nowal"
mkdir -p "$WALON/wal" "$WALOFF" "$NOWAL"; : > "$WALON/wal/docker-compose.wal.yml"
printf 'DOMAIN=myshop.co.kr\nWAL_ARCHIVE_MODE=on\n' > "$WALON/.env"
printf 'DOMAIN=myshop.co.kr\nWAL_ARCHIVE_MODE=off\n' > "$WALOFF/.env"
printf 'DOMAIN=myshop.co.kr\nWAL_ARCHIVE_MODE=on\n' > "$NOWAL/.env"
# shellcheck source=/dev/null # the CLI script under test is passed in
dc_args() { ( DAHAE_SELFTEST=1 . "$1"; cd "$2"; docker() { echo "$*"; }; dc ps ) 2>/dev/null; } # dc_args 스크립트 폴더 : 실제로 넘어갈 인자
WAL_ARGS="compose -f docker-compose.yml -f wal/docker-compose.wal.yml --env-file .env ps"
PLAIN_ARGS="compose -f docker-compose.yml --env-file .env ps"
t  "install.sh dc adds the wal override when archiving is on" eq "$(dc_args "$ROOT/deploy/install.sh" "$WALON")" "$WAL_ARGS"
t  "dahae dc adds the wal override when archiving is on" eq "$(dc_args "$SB/dahae-cron" "$WALON")" "$WAL_ARGS"
t  "archiving off: plain compose call" eq "$(dc_args "$SB/dahae-cron" "$WALOFF")" "$PLAIN_ARGS"
nt "archiving on without wal/ refuses to start" dc_args "$SB/dahae-cron" "$NOWAL"
printf 'WAL_ARCHIVE_MODE=on\n' >> "$SB/.env"; mkdir -p "$SB/wal"; : > "$SB/wal/base-backup.sh"
cron_cli cmd_write_cron
t  "archiving on: weekly base backup under the lock" grep -qF "15 2 * * 0 root cd $SB && flock -w 3600 /run/dahae.lock bash wal/base-backup.sh" "$SB/cron-out"
sed -i '/^WAL_ARCHIVE_MODE=/d' "$SB/.env"; cron_cli cmd_write_cron
nt "archiving off: no base backup line" grep -qF "base-backup.sh" "$SB/cron-out"

# auto-update: a skip mark, a waiting job, and the 24-hour wait for a new stable digest each skip the update
mkdir -p "$SB/host" "$SB/jobs/inbox"
printf 'CHANNEL=stable\nIMAGE_REPO=ghcr.io/owner\n' > "$SB/.env"
FAKE_MANIFEST='{"v":1}'
touch "$SB/host/skip-next-update"
nt "skip mark skips the update"          has_updated "$(auto_run)"
t  "skip mark is used up"                test ! -e "$SB/host/skip-next-update"
touch "$SB/jobs/inbox/waiting.job"
nt "a waiting job defers the update"     has_updated "$(auto_run)"
rm -f "$SB/jobs/inbox/waiting.job"
nt "a new stable digest waits 24 hours"  has_updated "$(auto_run)"
key=$(printf '%s\n%s\n' "$FAKE_MANIFEST" "$FAKE_MANIFEST" | sha256sum | cut -c1-64)
printf '%s %s\n' "$key" "$(( $(date +%s) - 90000 ))" > "$SB/host/first-seen"
t  "seen 25 hours ago: the update runs"  has_updated "$(auto_run)"
t  "update records the running digest"   eq "$(cat "$SB/host/applied-key")" "$key"
rm -f "$SB/host/first-seen"
t  "running digest: no 24-hour wait"     has_updated "$(auto_run)"
rm -f "$SB/host/applied-key" "$SB/host/first-seen"
nt "unreadable web manifest: no update"  has_updated "$(WEB_FAIL=1 auto_run)"
t  "unreadable web manifest: exit 1 so the alert runs" eq "$(WEB_FAIL=1 auto_run >/dev/null; echo $?)" 1
# after a rollback: the channel still at the version rolled back from is held; a newer one updates again
printf 'CHANNEL=stable\nVERSION=stable\nIMAGE_REPO=ghcr.io/owner\n' > "$SB/.env"; printf '%s\n' "$key" > "$SB/host/applied-key"
printf '%s\n' "$key" > "$SB/host/rollback-hold"
out=$(auto_run)
nt "rollback hold: the version rolled back from is not applied again" has_updated "$out"
t  "...and the run says why"             eq "$(printf '%s\n' "$out" | grep -c '되돌린 버전을 지켜요')" 1
printf 'CHANNEL=beta\nVERSION=beta\nIMAGE_REPO=ghcr.io/owner\n' > "$SB/.env"; FAKE_MANIFEST='{"v":2}'
t  "rollback hold: a newer version updates again" has_updated "$(auto_run)"
nt "rollback hold: an unreadable registry does not update" has_updated "$(WEB_FAIL=1 auto_run)"
FAKE_MANIFEST='{"v":1}'; rm -f "$SB/host/rollback-hold" "$SB/host/applied-key"
# after 'dahae update <tag>' (console, DODA's deploy job): back on the channel once it reaches that tag; a pin ahead of it waits
pin_env() { printf 'CHANNEL=stable
VERSION=%s
IMAGE_REPO=ghcr.io/owner
DELIVERY_RAW=https://raw.example/d
IMAGE_SOURCE=%s
' "$1" "${2:-remote}" > "$SB/.env"; printf '%s
' "$key" > "$SB/host/applied-key"; rm -f "$SB/curl-auto.log"; }
pin_env v1.0.0; out=$(CHAN_VER=v1.2.0 auto_run)
t  "pinned below the channel: back on the channel" grep -qx 'VERSION=stable' "$SB/.env"
t  "...and it updates"                   has_updated "$out"
t  "...after reading the channel's VERSION" grep -qF 'https://raw.example/d/main/VERSION' "$SB/curl-auto.log"
pin_env v1.2.0; CHAN_VER=v1.2.0 auto_run >/dev/null
t  "pinned at the channel's version: back on the channel" grep -qx 'VERSION=stable' "$SB/.env"
pin_env v2.0.0; out=$(CHAN_VER=v1.2.0 auto_run)
nt "pinned ahead of the channel: no update" has_updated "$out"
t  "...and the pin stays"                grep -qx 'VERSION=v2.0.0' "$SB/.env"
pin_env v1.0.0; out=$(auto_run); rc=$?
nt "channel VERSION unreadable: no update" has_updated "$out"
t  "...exit 1 so the alert runs"         eq "$rc" 1
t  "...and the pin stays"                grep -qx 'VERSION=v1.0.0' "$SB/.env"
pin_env v1.0.0 local; out=$(auto_run)
t  "local images: the pin stays"         grep -qx 'VERSION=v1.0.0' "$SB/.env"
t  "...and the update runs as before"    has_updated "$out"
nt "...without asking the delivery repo" test -e "$SB/curl-auto.log"
rm -f "$SB/host/applied-key"

# setup code: shown in groups of four for 7 days, then only --renew gives a new one
printf 'SETUP_CODE=ABCDEFGHJKMN\nSETUP_CODE_AT=%s\n' "$(date +%s)" > "$SB/.env"
t  "setup code shown in groups"          eq "$(setup_out)" ABCD-EFGH-JKMN
printf 'SETUP_CODE=ABCDEFGHJKMN\nSETUP_CODE_AT=%s\n' "$(( $(date +%s) - 200000 ))" > "$SB/.env"
t  "setup code still valid after 2 days" eq "$(setup_out)" ABCD-EFGH-JKMN
printf 'SETUP_CODE=ABCDEFGHJKMN\nSETUP_CODE_AT=%s\n' "$(( $(date +%s) - 700000 ))" > "$SB/.env"
nt "setup code expires after 7 days"     setup_out
# a code the owner pastes from the install page (DAHAE_SETUP_CODE): 12 characters, no 0 O 1 I L
t  "setup code from install page"        valid_setup_code ABCD2345678J
nt "setup code with zero"                valid_setup_code 0BCD2345678J
nt "setup code lowercase"                valid_setup_code abcd2345678j
nt "setup code too short"                valid_setup_code ABCD2345678
nt "setup code with dash"                valid_setup_code ABCD-2345678
t  "renew gives a 12-character code"     eq "$(setup_out --renew | tail -n1 | tr -d '\n-' | wc -c | tr -d ' ')" 12

# MALKKEUM_SETUP_CODE: the fresh .env keeps the page's code; a bad code stops the installer before it changes anything
fresh_env() { # fresh_env 코드 : write_fresh_env 가 만든 .env 의 설치 코드 줄
  local d; d=$(mktemp -d "$SB/fresh.XXXX")
  ( cd "$d" && MALKKEUM_SETUP_CODE="$1" write_fresh_env >/dev/null 2>&1 && grep '^SETUP_CODE=' .env ) || true
}
# shellcheck disable=SC2030 # the change is meant to stay in this subshell
code_env() { ( MALKKEUM_SETUP_CODE="$1"; check_setup_code_env ) >/dev/null 2>&1; }
t  "fresh .env keeps the page code"      eq "$(fresh_env ABCD2345678J)" SETUP_CODE=ABCD2345678J
t  "fresh .env makes a code without one" eq "$(fresh_env '' | awk '{ print length($0) }')" 23
t  "install accepts the page code"       code_env ABCD2345678J
nt "install refuses a code with zero"    code_env 0BCD2345678J
nt "install refuses a short code"        code_env ABCD2345678
no_code_env() { ( unset MALKKEUM_SETUP_CODE; check_setup_code_env ); }
t  "install without a code is fine"      no_code_env

# MALKKEUM_ANSWERS: only DOMAIN and EMAIL are read, values hold no spaces, environment values win
printf 'DOMAIN=answers.example.kr\nEMAIL=owner@example.kr\nEVIL=1\n' > "$SB/answers"
unset MALKKEUM_DOMAIN MALKKEUM_EMAIL MALKKEUM_EVIL
load_answers "$SB/answers"
t  "answers: domain"                     eq "$MALKKEUM_DOMAIN" answers.example.kr
t  "answers: email"                      eq "$MALKKEUM_EMAIL" owner@example.kr
t  "answers: other keys ignored"         eq "${MALKKEUM_EVIL:-}" ""
# shellcheck disable=SC2030 # the change is meant to stay in this subshell
t  "answers: environment wins"           eq "$(export MALKKEUM_DOMAIN=keep.example.kr; load_answers "$SB/answers"; printf '%s' "$MALKKEUM_DOMAIN")" keep.example.kr
printf 'DOMAIN=a b.kr\n' > "$SB/answers-bad"
nt "answers: space in value refused"     sub load_answers "$SB/answers-bad"
unset MALKKEUM_DOMAIN MALKKEUM_EMAIL

# the agent is checked against the fingerprint pinned in install.sh
printf 'echo agent\n' > "$SB/agent.sh"
pin=$(sha256sum "$SB/agent.sh" | cut -d' ' -f1)
t  "agent matches its pinned sha256"     agent_sha_ok "$SB/agent.sh" "$pin"
nt "agent without a pin refused"         agent_sha_ok "$SB/agent.sh" ""
printf 'echo changed\n' > "$SB/agent.sh"
nt "changed agent refused"               agent_sha_ok "$SB/agent.sh" "$pin"

# systemd units: path (inbox), service (root, sandboxed, 7 h limit), timer (every minute)
mkdir -p "$SB/units"; write_units "$SB/units"
t  "unit: path watches the inbox"        grep -qF "PathExistsGlob=$SB/jobs/inbox/*.job" "$SB/units/dahae-agent.path"
t  "unit: service runs the agent"        grep -qF "ExecStart=/bin/bash $SB/agent.sh run" "$SB/units/dahae-agent.service"
t  "unit: service sandboxed"             grep -qF "ProtectSystem=full" "$SB/units/dahae-agent.service"
t  "unit: timer every minute"            grep -qF "OnCalendar=*-*-* *:*:00" "$SB/units/dahae-agent.timer"
t  "unit: path also watches approvals"   grep -qF "PathExistsGlob=$SB/jobs/inbox/*.approve" "$SB/units/dahae-agent.path"
# job limits: a long job may run 6 h; a queued job waits for the lock at least that long. One run = lock wait,
# the host refresh (a normal job at most) and one job, so the unit limit must cover all three
read -r run_t upd_t lock_w < <( (eval "$(grep -E '^(RUN_TIMEOUT|UPDATE_RUN_TIMEOUT|LOCK_WAIT)=' "$ROOT/deploy/agent.sh")"; echo "$RUN_TIMEOUT $UPDATE_RUN_TIMEOUT $LOCK_WAIT") )
unit_h=$(sed -n 's/^TimeoutStartSec=\([0-9][0-9]*\)h$/\1/p' "$SB/units/dahae-agent.service")
t  "agent: normal job keeps 30 minutes"  eq "$run_t" 1800
t  "agent: update may run 6 hours"       eq "$upd_t" 21600
t  "agent: lock wait covers the update"   [ "$lock_w" -ge "$upd_t" ]
t  "unit: limit is set in hours"          [ -n "$unit_h" ]
t  "unit: limit covers lock wait + refresh + longest job" [ $((${unit_h:-0} * 3600)) -ge $((lock_w + run_t + upd_t)) ]

# server approval key: made once, private, and owner-reset removes it (the CLI runs in the sandbox)
mkdir -p "$SB/host"
STAFF=0190a6f2-1111-7000-8000-000000000001
enroll() { ( DAHAE_SELFTEST=1 . "$SB/dahae"; cd "$SB"; cmd_host_approval enroll "$1" ) >/dev/null 2>&1; }
nt "approval enroll without an owner refused" enroll ""
enroll "$STAFF"
t  "approval enroll writes a 32-character key" eq "$(wc -c < "$SB/host/approval.key" | tr -d ' ')" 33
t  "approval key is private"             eq "$(stat -c %a "$SB/host/approval.key")" 600
t  "approval enroll records the owner"   grep -qF "\"staff_id\":\"$STAFF\"" "$SB/host/owner.json"
nt "approval enroll twice refused"       enroll "$STAFF"
reset_owner() { ( DAHAE_SELFTEST=1 . "$SB/dahae"; cd "$SB"; YES=1 cmd_owner_reset ) >/dev/null 2>&1; }
printf '%s\n' "$(date +%s)" > "$SB/host/approval.fails"
reset_owner
t  "owner-reset removes the approval key" test ! -e "$SB/host/approval.key"
t  "owner-reset removes the owner record" test ! -e "$SB/host/owner.json"
t  "owner-reset clears the failure log"   test ! -e "$SB/host/approval.fails"

# version history: a version label is recorded once; a label that is not a version is skipped
# shellcheck disable=SC2034 # NEW_API is read by the sourced record_version
record_label() { ( DAHAE_SELFTEST=1 . "$SB/dahae"; cd "$SB"; docker() { echo "$LABEL"; }; NEW_API=img; LABEL="$1"; record_version ) >/dev/null 2>&1; }
rm -f "$SB/host/version-history"
record_label 2.1.0; record_label 2.1.0; record_label 2.1.0-beta.1; record_label dev
t  "record_version writes v2.1.0 once"        eq "$(grep -c '^v2\.1\.0$' "$SB/host/version-history")" 1
t  "record_version writes the beta tag"       grep -qxF v2.1.0-beta.1 "$SB/host/version-history"
t  "record_version skips a non-version label"  eq "$(grep -c -v '^v[0-9]' "$SB/host/version-history")" 0

# compose mounts the job spool into the API and the progress folder into the web container
t  "api mounts inbox read-write"         grep -qF "/opt/malkkeum/jobs/inbox:/jobs/inbox," "$ROOT/deploy/docker-compose.yml"
t  "api mounts status and out read-only" grep -qF "/opt/malkkeum/jobs/status:/jobs/status:ro, /opt/malkkeum/jobs/out:/jobs/out:ro]" "$ROOT/deploy/docker-compose.yml"
t  "web mounts progress read-only"       grep -qF ":/srv/ops-progress:ro]" "$ROOT/deploy/docker-compose.yml"
t  "Caddy progress route is no-store"    grep -qF 'Cache-Control "no-store"' "$ROOT/deploy/Caddyfile"
# edge logs: rotated JSON access log, secrets in query strings redacted, probes answer 404 publicly
t  "Caddy access log rotates 14 files"   grep -qF "roll_keep 14" "$ROOT/deploy/Caddyfile"
t  "Caddy access log drops every query string and the ops-progress token" grep -qF '(ops-progress/|[?]).*' "$ROOT/deploy/Caddyfile"
t  "Caddy access log keeps 3 months at most" grep -qF "roll_keep_for 2160h" "$ROOT/deploy/Caddyfile"
t  "Caddy access log drops the owner code header" grep -qF "request>headers>X-Owner-Totp delete" "$ROOT/deploy/Caddyfile"
t  "Caddy access log drops Referer"     grep -qF "request>headers>Referer delete" "$ROOT/deploy/Caddyfile"
t  "Caddy /ready and /metrics answer 404" grep -qF "@internal path /ready /metrics" "$ROOT/deploy/Caddyfile"
t  "Caddy answers 404 for them in a handle block" grep -qF "handle @internal {" "$ROOT/deploy/Caddyfile"
t  "uptime-kuma is opt-in and loopback-only" grep -qF 'profiles: [monitoring]' "$ROOT/deploy/docker-compose.yml"
t  "embedded compose has the access log volume" grep -qF "caddylogs:/var/log/caddy" "$SB/compose/docker-compose.yml"
# the agent is installed on a fresh install and on an update: install_agent is called from main, not from a branch
t  "install_agent runs in main"          grep -qE '^  install_agent( |$)' "$ROOT/deploy/install.sh"
# shellcheck disable=SC2016 # literal text searched in the installer
t  "cron table written by the CLI"       grep -qF '"$BIN/dahae" write-cron' "$ROOT/deploy/install.sh"
# shellcheck disable=SC2016 # literal text searched in the publisher
t  "publish fills RELEASE_RAW"           grep -qF 'RELEASE_RAW=$RAW/$ref' "$ROOT/deploy/publish-delivery.sh"

# ---- host hardening (SEC-9): firewall, fail2ban, unattended upgrades, key-only SSH ----
HB="$SB/hardening"; mkdir -p "$HB"
fw() { ( MALKKEUM_FIREWALL="$1"; firewall_wanted ); }
t  "sshd -T reports password login"        password_login_on "$(printf 'port 22\npasswordauthentication yes\n')"
nt "sshd -T with password login off"       password_login_on "$(printf 'passwordauthentication no\n')"
t  "firewall on when MALKKEUM_FIREWALL=on" fw on
nt "firewall stays off when MALKKEUM_FIREWALL=off" fw off
t  "fail2ban jail enables sshd"            write_f2b_jail "$HB/jail.local"
t  "fail2ban jail text"                    grep -qx '\[sshd\]' "$HB/jail.local"
t  "unattended upgrades on"                write_auto_upgrades "$HB/auto"
t  "unattended upgrades text"              grep -qx 'APT::Periodic::Unattended-Upgrade "1";' "$HB/auto"
t  "key-only drop-in"                      write_keyonly_dropin "$HB/keyonly.conf"
t  "key-only drop-in text"                 grep -qx 'PasswordAuthentication no' "$HB/keyonly.conf"
: > "$HB/empty-keys"; printf 'ssh-ed25519 AAAA test\n' > "$HB/keys"
nt "empty authorized_keys is not a key"   has_key "$HB/empty-keys"
t  "authorized_keys with a key"            has_key "$HB/no-such" "$HB/keys"
t  "install installs ufw fail2ban unattended-upgrades" grep -qF 'ufw fail2ban unattended-upgrades' "$ROOT/deploy/install.sh"
# shellcheck disable=SC2016 # literal text on purpose
t  "install runs harden_host in main"      grep -qE '^  \[ "\$TOOLS_ONLY" = 1 \] \|\| harden_host$' "$ROOT/deploy/install.sh"
# behavior: firewall_setup and ssh_keyonly_check with stubbed ufw, sshd, ask, systemctl (no real /etc or firewall changes)
FWLOG="$HB/fw.log"
fw_run() { # fw_run 방화벽상태 모드 : firewall_setup 을 가짜 ufw 로 돌리고 실행한 명령을 fw.log 에 남겨요
  : > "$FWLOG"
  # shellcheck disable=SC2030 # the change is meant to stay in this subshell
  ( UFW_STATUS="$1"; MALKKEUM_FIREWALL="$2"
    ufw() { if [ "$1" = status ]; then printf '%s\n' "$UFW_STATUS"; else printf '%s\n' "$*" >> "$FWLOG"; fi; }
    firewall_setup ) >/dev/null 2>&1
}
fw_enabled()   { grep -qF -- '--force enable' "$FWLOG"; }
fw_web_ports() { grep -qF -- 'allow 22/tcp' "$FWLOG" && grep -qF -- 'allow 80/tcp' "$FWLOG" && grep -qF -- 'allow 443/tcp' "$FWLOG"; }
fw_ssh_first() { awk '/allow 22\/tcp/ { a = NR } /--force enable/ { e = NR } END { exit !(a && e && a < e) }' "$FWLOG"; }
fw_run 'Status: inactive' on;  t  "inactive ufw is enabled when MALKKEUM_FIREWALL=on" fw_enabled
fw_run 'Status: inactive' on;  t  "web ports are allowed before ufw is enabled" fw_ssh_first
fw_run 'Status: inactive' on;  t  "ports 22 80 443 are allowed on enable" fw_web_ports
fw_run 'Status: active' on;    nt "active ufw is not enabled again" fw_enabled
fw_run 'Status: active' on;    t  "active ufw gets the web rules" fw_web_ports
fw_run 'Status: inactive' off; nt "inactive ufw stays off when MALKKEUM_FIREWALL=off" fw_enabled
ssh_run() { # ssh_run sshd설정 키파일 응답 : ssh_keyonly_check 를 가짜 sshd·ask 로 돌려요 (기록은 fw.log, 출력은 ssh.out)
  : > "$FWLOG"
  ( SSHD_CFG="$1"; KEYF="$2"; ASK_ANSWER="$3"
    sshd() { if [ "$1" = -T ]; then [ -z "${SSHD_FAIL:-}" ] || return 1; printf '%s\n' "$SSHD_CFG"; fi; return 0; }
    login_key_file() { printf '%s' "$KEYF"; }
    ask() { printf '%s\n' "$1" >> "$FWLOG"; printf '%s' "$ASK_ANSWER"; }
    write_keyonly_dropin() { printf 'dropin %s\n' "$1" >> "$FWLOG"; }
    systemctl() { return 0; }
    KEYONLY_CONF="$HB/keyonly-test.conf"
    ssh_keyonly_check ) >"$HB/ssh.out" 2>&1
}
dropin_written() { grep -q '^dropin ' "$FWLOG"; }
PW_ON='passwordauthentication yes'
ssh_run "$PW_ON" "$HB/keys" y;         t  "key on the operator account and answer y turns password login off" dropin_written
SSHD_FAIL=1 ssh_run "$PW_ON" "$HB/keys" y; nt "sshd -T failure never turns password login off" dropin_written
SSHD_FAIL=1 ssh_run "$PW_ON" "$HB/keys" y; t  "sshd -T failure is reported" grep -qF '읽지 못해' "$HB/ssh.out"
ssh_run "$PW_ON" "$HB/none" y;         nt "no key for the account: password login stays on" dropin_written
ssh_run "$PW_ON" "$HB/keys" n;         nt "answer n keeps password login" dropin_written
SUDO_USER=alice ssh_run "$PW_ON" "$HB/keys" y; t "prompt names the operator account" grep -qF "'alice'" "$FWLOG"
eq "$(SUDO_USER=alice operator_account)" alice
eq "$(SUDO_USER='' operator_account)" root
eq "$(login_key_file root)" /root/.ssh/authorized_keys
nt "odd account name refused for key lookup" login_key_file 'a;rm'

# ---- p07: restore resets the schema first; re-run updates once the images are set; prune keeps the rollback image; photo offsite failures warn ----
mkdir -p backups
printf 'DROP TABLE IF EXISTS staff;\nCREATE TABLE staff (id int);\n' | gzip -c > backups/db-p07.sql.gz
restore_first_line() { restore_stream backups/db-p07.sql.gz | head -n1; }
restore_has() { restore_stream backups/db-p07.sql.gz | grep -F "$1" >/dev/null; } # no -q: pipefail would see SIGPIPE
t  "restore resets the public schema first"  grep -qF 'DROP SCHEMA public CASCADE' <(restore_first_line)
t  "restore re-creates the app extensions"   restore_has 'CREATE EXTENSION IF NOT EXISTS pgcrypto'
t  "restore keeps the dump after the reset"  restore_has 'CREATE TABLE staff'
printf 'DOMAIN=myshop.co.kr\nAPI_IMAGE=repo/malkkeum-api@sha256:aa\n' > .env
t  "re-run with stopped DB updates"          eq "$(install_mode 0)" update
t  "first run installs"                      eq "$(install_mode 1)" fresh
printf 'DOMAIN=myshop.co.kr\nDB_PASSWORD=x\n' > .env # the first run died before prepare set the images
t  "re-run after a failed first install installs" eq "$(install_mode 0)" fresh
printf 'DOMAIN=myshop.co.kr\nAPI_IMAGE=\n' > .env
t  "empty API_IMAGE installs"                eq "$(install_mode 0)" fresh
printf 'API_IMAGE=old\nWEB_IMAGE=old\n' > .env
( docker() { case "$*" in
    "image inspect -f {{.Id}} repo/x:v1") echo sha256:abcdef0123456789abcdef ;;
    "image inspect -f {{index .RepoDigests 0}} sha256:abcdef0123456789abcdef") echo "repo/malkkeum-api@sha256:ff" ;;
    tag*) echo "$*" >> "$SB/tags.log" ;;
  esac; }
  digest_ref repo/x:v1 repo/malkkeum-api > "$SB/digest.out" )
t  "rollback image gets a local keep tag"    eq "$(cat "$SB/digest.out")" "repo/malkkeum-api:keep-abcdef012345"
t  "...and it is tagged so prune keeps it"   grep -qF "tag sha256:abcdef0123456789abcdef repo/malkkeum-api:keep-abcdef012345" "$SB/tags.log"
set_images_safe() { ( docker() { case "$*" in "image inspect"*|"pull "*) return 1 ;; esac; return 0; }; set_images "$@" ) >/dev/null 2>&1; }
nt "missing image stops set_images"          set_images_safe repo/x:keep-1 repo/y
t  ".env keeps the old image when pull fails" eq "$(env_get API_IMAGE)" old
# an update stopped (time limit, shutdown) after the new images are up puts the old ones back; a finished one does not
update_then_term() { # update_then_term during|after : TERM while waiting for health, or once the update is done
  : > "$SB/set.log"
  ( mode=$1; db_roles() { :; }; cmd_backup() { :; }; prune_keep_tags() { :; }; record_version() { :; }
    # shellcheck disable=SC2034 # read by the sourced cmd_update
    pull_images() { NEW_API=repo/api:new; NEW_WEB=repo/web:new; }
    running_image() { echo "$2"; }; digest_ref() { echo "$1"; }; docker() { echo "id-$*"; }
    set_images() { echo "$1 $2" >> "$SB/set.log"; }
    wait_healthy() { [ "$mode" = after ] || kill -TERM "$BASHPID"; return 0; }
    cmd_update; [ "$mode" = during ] || kill -TERM "$BASHPID" ) >/dev/null 2>&1
  tail -n1 "$SB/set.log"
}
t  "update stopped before health check rolls back" eq "$(update_then_term during)" "old old"
t  "finished update keeps the new images"   eq "$(update_then_term after)" "repo/api:new repo/web:new"
# the stack was down with no images in .env: a stop during the health check must not write empty images
update_then_term_unknown() { local keep; keep=$(cat .env); : > .env; update_then_term during; printf '%s' "$keep" > .env; }
t  "stop with no previous images leaves .env alone" eq "$(update_then_term_unknown)" "repo/api:new repo/web:new"
# rollback waits while a Toss payout may still be at Toss in a way the previous release cannot handle
rollback_gate() { ( dc() { [ "$1" = exec ] || return 0; [ "$GATE" = fail ] && return 1; printf '%s\n' "$GATE"; }; GATE=$1; payouts_settled_for_rollback ) >/dev/null 2>&1; }
t  "rollback runs with no payout in doubt"   rollback_gate 0
nt "rollback refused while a payout is in doubt" rollback_gate 2
nt "rollback refused when the check cannot run" rollback_gate fail
nt "rollback refused on an odd answer"       rollback_gate 'ERROR'
backup_warn_after() { ( api_backup() { case "$*" in backup-seal*) [ -f "${2#/}" ] && printf 'MKB1' > "${3#/}"; return 0 ;; *"backup-upload /backups/uploads-"*) return 1 ;; *) return 0 ;; esac; }
  dc() { case "$1" in exec) printf 'dump' ;; run) printf 'tar' ;; esac; return 0; }
  cmd_backup >/dev/null 2>&1; printf '%s' "$BACKUP_WARN" ); }
t  "photo offsite failure sets the warning"  eq "$(backup_warn_after)" 1
# local retention: DB dumps 14 days; photo archives 3 days only when this run's photos reached offsite storage
# backup_keeps_after 외부(uploaded|unset|fail) : 오래된 사본을 둔 뒤 백업하고 남은 이름을 돌려줘요
backup_keeps_after() { ( OFF=$1; rm -f backups/db-old* backups/uploads-old* backups/backup.log
  for n in db-old5.sql.gz.enc uploads-old5.tar.gz.enc db-old20.sql.gz.enc uploads-old20.tar.gz.enc; do
    : > "backups/$n"; d=${n#*old}; touch -d "${d%%.*} days ago" "backups/$n"
  done
  api_backup() { case "$*" in backup-seal*) [ -f "${2#/}" ] && printf 'MKB1' > "${3#/}"; return 0 ;;
    "backup-upload /backups/uploads-"*) case "$OFF" in uploaded) echo "offsite backup uploaded: ${2##*/}" ;; unset) echo "offsite backup not set up (console: 설정 > 백업)" ;; fail) return 1 ;; esac ;;
    *) return 0 ;; esac; }
  dc() { case "$1" in exec) printf 'dump' ;; run) printf 'tar' ;; esac; return 0; }
  cmd_backup >/dev/null 2>&1; cd backups && for f in db-old* uploads-old*; do [ -e "$f" ] && printf '%s ' "$f"; done ); }
t  "offsite photos: 3 local photo days, 14 DB days" eq "$(backup_keeps_after uploaded)" "db-old5.sql.gz.enc "
t  "no offsite: photos and DB both kept 14 days"    eq "$(backup_keeps_after unset)" "db-old5.sql.gz.enc uploads-old5.tar.gz.enc "
t  "offsite failed: photos kept 14 days"            eq "$(backup_keeps_after fail)" "db-old5.sql.gz.enc uploads-old5.tar.gz.enc "
t  "photo retention is a named constant"            eq "$PHOTO_KEEP_DAYS_OFFSITE $BACKUP_KEEP_DAYS" "3 14"
rm -f backups/db-old* backups/uploads-old*
rm -f .env backups/db-p07.sql.gz* backups/uploads-* 2>/dev/null

# ---- dahae domain: the new name is checked over HTTPS before anything changes; then .env backed up, DOMAIN set,
# health over the new name; no answer after the switch restores .env and exits 3 ----
# domain_run 응답(ok|fail|late|term) 도메인 : 가짜 dc·curl·getent 로 cmd_domain 을 돌려요. 종료 코드를 돌려줘요
# late: answers only before the switch; term: the job is stopped after the switch
# shellcheck disable=SC2034 # DOMAIN_WAIT_SECS and YES are read by the sourced CLI
domain_run() {
  ( DAHAE_SELFTEST=1 . "$SB/dahae"; cd "$SB"; YES=1; DOMAIN_WAIT_SECS=0; HEALTH=$1
    dc() { printf '%s\n' "$*" >> "$SB/dc.log"; }
    # BROKEN_RESTORE: the stack put back on the old domain does not come up
    wait_healthy() { [ -z "${BROKEN_RESTORE:-}" ] || ! grep -qx 'DOMAIN=old.co.kr' .env; }
    curl() { case "$*" in *api.ipify.org*) echo 203.0.113.7 ;;
      *"/health"*) printf '%s|%s\n' "$*" "$(cat jobs/out/domain-pending 2>/dev/null)" >> "$SB/curl.log"
        if grep -qx 'DOMAIN=old.co.kr' .env; then [ "$HEALTH" != fail ]; else
          [ "$HEALTH" != term ] || kill -TERM "$BASHPID"; [ "$HEALTH" = ok ]; fi ;;
      *) return 1 ;; esac; }
    getent() { [ "$1" != ahostsv4 ] || echo "${DNS_IP:-203.0.113.7}  STREAM $2"; }
    cmd_domain "$2" ) >/dev/null 2>&1
}
domain_env() { printf 'DOMAIN=old.co.kr\nJWT_SECRET=keep\n' > "$SB/.env"; chmod 600 "$SB/.env"; rm -f "$SB"/backups/env-before-domain-* "$SB/dc.log" "$SB/curl.log" "$SB/jobs/out/domain-rolled-back"; }
domain_env; domain_run ok new.co.kr; rc=$?
t  "domain: success exits 0"               eq "$rc" 0
t  "domain: DOMAIN is the new one"         grep -qx 'DOMAIN=new.co.kr' "$SB/.env"
t  "domain: other keys stay"               grep -qx 'JWT_SECRET=keep' "$SB/.env"
t  "domain: .env backed up first"          grep -qx 'DOMAIN=old.co.kr' "$SB"/backups/env-before-domain-*
t  "domain: backup is root-only"           eq "$(stat -c %a "$SB"/backups/env-before-domain-*)" 600
t  "domain: api and web recreated"         grep -qx 'up -d api web' "$SB/dc.log"
t  "domain: checked before the switch, with a certificate allowed" grep -q 'https://new.co.kr/health|new.co.kr$' "$SB/curl.log"
t  "domain: checked on this server, not through NAT" grep -q -- '--resolve new.co.kr:443:127.0.0.1' "$SB/curl.log"
nt "domain: the certificate allowance is removed" test -e "$SB/jobs/out/domain-pending"
domain_env; domain_run fail new.co.kr; rc=$?
t  "domain: no answer before the switch is refused" eq "$rc" 1
t  "...with .env untouched"                grep -qx 'DOMAIN=old.co.kr' "$SB/.env"
nt "...and no restart"                     test -e "$SB/dc.log"
nt "...and no backup"                      ls "$SB"/backups/env-before-domain-* >/dev/null 2>&1
nt "...and the allowance removed"          test -e "$SB/jobs/out/domain-pending"
domain_env; domain_run late new.co.kr; rc=$?
t  "domain: no answer after the switch exits 3 (rolled back)" eq "$rc" 3
t  "domain: .env restored"                 grep -qx 'DOMAIN=old.co.kr' "$SB/.env"
t  "domain: restarted twice"               eq "$(grep -c 'up -d api web' "$SB/dc.log")" 2
t  "domain: the API is told which host failed" grep -qx new.co.kr "$SB/jobs/out/domain-rolled-back"
domain_env; BROKEN_RESTORE=1 domain_run late new.co.kr; rc=$?
t  "domain: a restore that does not answer is a failure, not rolled back" eq "$rc" 1
t  "...and .env is still restored"         grep -qx 'DOMAIN=old.co.kr' "$SB/.env"
domain_env; domain_run term new.co.kr; rc=$?
t  "domain: stopped mid-move exits 143"    eq "$rc" 143
t  "domain: stopped mid-move restores .env" grep -qx 'DOMAIN=old.co.kr' "$SB/.env"
domain_env; domain_run ok old.co.kr; rc=$?
t  "domain: the current one is refused"    eq "$rc" 1
nt "domain: refusal makes no backup"       ls "$SB"/backups/env-before-domain-* >/dev/null 2>&1
domain_env; domain_run ok localhost; rc=$?
t  "domain: a bad name is refused"         eq "$rc" 1
domain_env; DNS_IP=198.51.100.9 domain_run ok new.co.kr; rc=$?
t  "domain: DNS elsewhere is refused"      eq "$rc" 1
t  "domain: .env untouched on refusal"     grep -qx 'DOMAIN=old.co.kr' "$SB/.env"
nt "domain: no restart on refusal"         test -e "$SB/dc.log"
rm -rf "$SB/.env" "$SB"/backups/env-before-domain-* "$SB/dc.log" "$SB/curl.log" "$SB/jobs"

# ---- dahae update refreshes the host tools from the delivery repo: signed SHA256SUMS, matching install.sh and agent.sh, else stop ----
cd "$SB"; mkdir -p "$SB/host" "$SB/bin"
cat > "$SB/bin/cosign" <<'COS'
#!/usr/bin/env bash
printf '%s\n' "$*" > "$(dirname "$0")/blob-args"
[ "${FAKE_BLOB:-ok}" = ok ]
COS
chmod 755 "$SB/bin/cosign"
mkrel() { # mkrel 태그 [old] : 가짜 배포본 (install.sh·agent.sh, 지문 파일, 서명 묶음). old: 관리 도구 모드를 모르는 설치 프로그램
  # shellcheck disable=SC2016 # literal text on purpose
  local d="$SB/rel/$1" mode='# [ "${MALKKEUM_HOST_UPDATE:-}" != tools ]'; mkdir -p "$d"
  [ "${2:-}" != old ] || mode='# full install only'
  printf '#!/usr/bin/env bash\necho "agent %s"\n' "$1" > "$d/agent.sh"
  printf '#!/usr/bin/env bash\n%s\nAGENT_SHA256=%s\necho "tools %s"\n' "$mode" "$(sha256sum "$d/agent.sh" | cut -d' ' -f1)" "$1" > "$d/install.sh"
  (cd "$d" && sha256sum install.sh agent.sh > SHA256SUMS && printf 'bundle' > SHA256SUMS.bundle)
}
mkchan() { mkdir -p "$SB/rel/$1"; printf '%s\n' "$2" > "$SB/rel/$1/VERSION"; } # mkchan 가지 버전 : 채널 가지가 가리키는 태그 (publish-delivery 의 VERSION)
mkrel v1.7.0; mkrel v1.8.0; mkrel v1.5.0 old; mkchan main 1.8.0
tools_env() { printf 'DELIVERY_RAW=file://%s/rel\nSIGNER_REPO=EricWijesinghe/Dahae\nIMAGE_SOURCE=remote\nIMAGE_VERIFY=on\nIMAGE_REPO=repo\nAPI_IMAGE=old\nWEB_IMAGE=old\nVERSION=stable\n' "$SB" > "$SB/.env"; rm -rf "$SB/host"/*; rm -f "$SB/tools.log"; }
t  "tools ref: a tag stays"           eq "$(tools_ref v1.7.0)" v1.7.0
t  "tools ref: a bare tag gets its v" eq "$(tools_ref 1.7.0)" v1.7.0
t  "tools ref: stable is main"        eq "$(tools_ref stable)" main
t  "tools ref: beta is beta"          eq "$(tools_ref beta)" beta
t  "tools ref: odd names give none"   eq "$(tools_ref 'v1.7;id')$(tools_ref local)$(tools_ref v1.7)" ""
t  "version order: 1.7.9 is older than 1.8.0" ver_older v1.7.9 v1.8.0
nt "version order: 1.10.0 is not older than 1.9.0" ver_older v1.10.0 v1.9.0
nt "version order: a stable release is not older than its beta" ver_older v1.8.0 v1.8.0-beta.2
fetch() { ( DAHAE_SELFTEST=1 . "$SB/dahae"; cd "$SB"; fetch_installer "$1" host/next ) >/dev/null 2>&1; }
tools_env
t  "fetch: signed sums and matching install.sh pass" fetch v1.7.0
t  "fetch: verify-blob with the bundle"             grep -qF -- "verify-blob --bundle host/next/SHA256SUMS.bundle" "$SB/bin/blob-args"
t  "fetch: signed at exactly this tag"              grep -qF 'EricWijesinghe/Dahae/\.github/workflows/release\.yml@refs/tags/v1\.7\.0$' "$SB/bin/blob-args"
t  "fetch: the GitHub OIDC issuer"                  grep -qF -- "--certificate-oidc-issuer https://token.actions.githubusercontent.com" "$SB/bin/blob-args"
t  "fetch: the agent of the same release is kept"   grep -qF 'agent v1.7.0' "$SB/host/next/agent.sh"
FAKE_BLOB=bad; export FAKE_BLOB
nt "fetch: a bad signature stops"                   fetch v1.7.0
FAKE_BLOB=ok
printf 'echo evil\n' >> "$SB/rel/v1.7.0/install.sh"
nt "fetch: an install.sh unlike SHA256SUMS stops"   fetch v1.7.0
mkrel v1.7.0; printf 'echo evil\n' >> "$SB/rel/v1.7.0/agent.sh"
nt "fetch: an agent.sh unlike SHA256SUMS stops"     fetch v1.7.0
mkrel v1.7.0; sed -i 's/^AGENT_SHA256=.*/AGENT_SHA256=0000000000000000000000000000000000000000000000000000000000000000/' "$SB/rel/v1.7.0/install.sh"
(cd "$SB/rel/v1.7.0" && sha256sum install.sh agent.sh > SHA256SUMS)
nt "fetch: an agent.sh unlike the installer's pinned hash stops" fetch v1.7.0
mkrel v1.7.0; printf '%s  agent.sh\n' "$(printf '0%.0s' $(seq 64))" > "$SB/rel/v1.7.0/SHA256SUMS"
nt "fetch: no install.sh line stops"                fetch v1.7.0
mkrel v1.7.0; rm -f "$SB/rel/v1.7.0/SHA256SUMS.bundle"
nt "fetch: a missing bundle stops"                  fetch v1.7.0
mkrel v1.7.0
nt "fetch: an unknown release stops"                fetch v9.9.9
t  "fetch: the channel is read as its tag"          fetch main
t  "...and signed at that tag"                      grep -qF 'release\.yml@refs/tags/v1\.8\.0$' "$SB/bin/blob-args"
printf 'v1.9.0\n' > "$SB/host/version-history"
nt "fetch: a channel older than this server's newest version stops" fetch main
rm -f "$SB/host/version-history"; mkchan main 'v1.8;id'
nt "fetch: an odd channel version stops"            fetch main
mkchan main 1.8.0
t  "fetch: an installer without tools mode is told apart" eq "$( ( DAHAE_SELFTEST=1 . "$SB/dahae"; cd "$SB"; fetch_installer v1.5.0 host/next ) >/dev/null 2>&1; echo $?)" 2
# upd 버전 [HEALTH] : 가짜 백업·이미지로 cmd_update 를 돌려요. 단계는 tools.log 에, 종료 코드를 돌려줘요. SAME=1: 같은 이미지로 다시 맞추기
upd() {
  ( DAHAE_SELFTEST=1 . "$SB/dahae"; cd "$SB"; H=${2:-ok}
    log() { printf '%s\n' "$*" >> "$SB/tools.log"; }
    db_roles() { :; }; cmd_backup() { log backup; }; prune_keep_tags() { :; }; record_version() { :; }
    # shellcheck disable=SC2034 # read by the sourced cmd_update
    pull_images() { NEW_API=repo/api:new; NEW_WEB=repo/web:new; }
    running_image() { echo "$2"; }; digest_ref() { echo "$1"; }
    docker() { if [ -n "${SAME:-}" ]; then echo same; else echo "id-$*"; fi; }
    df() { printf 'Filesystem 1024-blocks Used Available Capacity Mounted
fs 1 1 %s 1%% /
' "${DISK_FREE:-999999999}"; }
    dc() { [ "$1" != run ] || echo "${UPLOADS_KB:-0} /data/uploads"; }
    set_images() { log "images $1"; }; wait_healthy() { [ "$H" = ok ] || [ "$(grep '^images' "$SB/tools.log" | tail -n1)" != "images repo/api:new" ]; }
    run_installer_tools() { [ -z "${TOOLS_FAIL:-}" ] || return 1; log "tools $(bash "$1") $(bash "$2")"; }
    cmd_update "$1" ) >/dev/null 2>&1
}
tools_env; printf 'x\n' > "$SB/host/rollback-hold"; upd v1.7.0; rc=$?
t  "update: ok"                                     eq "$rc" 0
t  "update: tools come after the new images"        eq "$(tail -n1 "$SB/tools.log")" "tools tools v1.7.0 agent v1.7.0"
t  "update: the verified installer is kept"         grep -qF 'tools v1.7.0' "$SB/host/install.sh"
t  "update: ...root-only"                           eq "$(stat -c %a "$SB/host/install.sh")" 600
t  "update: ...with its agent"                      grep -qF 'agent v1.7.0' "$SB/host/agent.keep.sh"
nt "update: no previous installer yet"              test -e "$SB/host/install.prev.sh"
nt "update: a chosen update ends a rollback hold"   test -e "$SB/host/rollback-hold"
upd stable
t  "update: the channel takes the tag it points at" eq "$(tail -n1 "$SB/tools.log")" "tools tools v1.8.0 agent v1.8.0"
t  "update: the last installer is kept for rollback" grep -qF 'tools v1.7.0' "$SB/host/install.prev.sh"
t  "update: ...and its agent"                       grep -qF 'agent v1.7.0' "$SB/host/agent.prev.sh"
SAME=1 upd stable; SAME=1 upd stable
t  "same image again (weekly auto-update): the rollback installer stays" grep -qF 'tools v1.7.0' "$SB/host/install.prev.sh"
t  "...and its agent stays"                         grep -qF 'agent v1.7.0' "$SB/host/agent.prev.sh"
rm -f "$SB/tools.log"; DAHAE_TOOLS_FRESH=1 upd v1.7.0
t  "install-line re-run with new images: the old installer becomes the rollback one" grep -qF 'tools v1.8.0' "$SB/host/install.prev.sh"
nt "...and none is kept for the tools the install line just wrote" test -e "$SB/host/install.sh"
nt "...and no second refresh"                       grep -q '^tools' "$SB/tools.log"
tools_env; FAKE_BLOB=bad upd v1.7.0; rc=$?
t  "update: a bad signature stops the update"       eq "$rc" 1
nt "...before the backup or any image"              test -e "$SB/tools.log"
tools_env; upd v1.7.0 fail; rc=$?
t  "update: a new version that does not answer exits 3" eq "$rc" 3
nt "...and the tools stay"                          grep -q '^tools' "$SB/tools.log"
nt "...and no installer is kept"                    test -e "$SB/host/install.sh"
tools_env; TOOLS_FAIL=1 upd v1.7.0; rc=$?
t  "update: new images fine but the tools refresh failed exits 5" eq "$rc:$(grep -c '^images' "$SB/tools.log")" 5:1
tools_env; upd v1.5.0; rc=$?
t  "update to a release whose installer has no tools mode keeps the tools" eq "$rc:$(grep -c '^tools' "$SB/tools.log")" 0:0
tools_env; env_set DELIVERY_RAW ''; upd v1.7.0; rc=$?
t  "update without a delivery address still updates" eq "$rc:$(grep -c '^images' "$SB/tools.log")" 0:1
nt "...with the tools as they are"                  grep -q '^tools' "$SB/tools.log"
tools_env; env_set IMAGE_SOURCE local; upd v1.7.0
nt "local images: tools as they are"                grep -q '^tools' "$SB/tools.log"
# disk: twice the photos (backup and its sealed copy) + 3 GB, on the install folder and Docker's, before anything changes
tools_env; env_set DELIVERY_RAW ''; DISK_FREE=1048576 upd v1.7.0; rc=$?
t  "update: a full disk stops it"                   eq "$rc" 1
nt "...before the backup or any image"              test -e "$SB/tools.log"
tools_env; env_set DELIVERY_RAW ''; UPLOADS_KB=2097152 DISK_FREE=7340031 upd v1.7.0; rc=$?
t  "update: the photos count twice in the space needed" eq "$rc" 1
tools_env; env_set DELIVERY_RAW ''; UPLOADS_KB=2097152 DISK_FREE=7340032 upd v1.7.0; rc=$?
t  "update: twice the photos + 3 GB free runs"      eq "$rc:$(grep -c '^images' "$SB/tools.log")" 0:1
# an automatic rollback holds the release it fled: no backup, outage and rollback again every week
tools_env; env_set DELIVERY_RAW ''; upd '' fail; rc=$?
t  "auto-rollback exits 3"                          eq "$rc" 3
t  "...and holds the channel's bad build"           eq "$(cat "$SB/host/rollback-hold" 2>/dev/null)" "$( ( DAHAE_SELFTEST=1 . "$SB/dahae"; cd "$SB"; docker() { echo "id-$*"; }; manifest_key stable ) )"
# rb [GATE] : 가짜 정산 확인(GATE=0 이면 통과)으로 cmd_rollback 을 돌려요
rb() {
  ( DAHAE_SELFTEST=1 . "$SB/dahae"; cd "$SB"; GATE=${1:-0}
    log() { printf '%s\n' "$*" >> "$SB/tools.log"; }
    cmd_backup() { log backup; }; running_image() { echo "$2"; }; digest_ref() { echo "$1"; }
    dc() { [ "$1" = exec ] && printf '%s\n' "$GATE"; return 0; }
    docker() { [ -z "${NO_MANIFEST:-}" ] || return 1; echo "manifest-$*"; }
    set_images() { log "images $1"; }; wait_healthy() { return 0; }
    run_installer_tools() { [ -z "${TOOLS_FAIL:-}" ] || return 1; log "tools $(bash "$1") $(bash "$2" 2>/dev/null)"; }
    cmd_rollback ) >/dev/null 2>&1
}
tools_env; env_set PREV_API_IMAGE prev; env_set PREV_WEB_IMAGE prevweb
install -m 600 "$SB/rel/v1.8.0/install.sh" "$SB/host/install.sh"; install -m 600 "$SB/rel/v1.7.0/install.sh" "$SB/host/install.prev.sh"
install -m 600 "$SB/rel/v1.8.0/agent.sh" "$SB/host/agent.keep.sh"; install -m 600 "$SB/rel/v1.7.0/agent.sh" "$SB/host/agent.prev.sh"
rb 2; rc=$?
t  "rollback: payouts in doubt exit 4"              eq "$rc" 4
nt "...with the images unchanged"                   grep -q '^images' "$SB/tools.log"
t  "...and the installers unchanged"                grep -qF 'tools v1.8.0' "$SB/host/install.sh"
rm -f "$SB/tools.log"; rb 0; rc=$?
t  "rollback: ok"                                   eq "$rc" 0
t  "rollback: the previous images first"            eq "$(grep '^images' "$SB/tools.log")" "images prev"
t  "rollback: the tools and agent follow the rolled-back version" eq "$(tail -n1 "$SB/tools.log")" "tools tools v1.7.0 agent v1.7.0"
t  "rollback: the newer installer is kept for a second rollback" grep -qF 'tools v1.8.0' "$SB/host/install.prev.sh"
t  "...with its agent"                              grep -qF 'agent v1.8.0' "$SB/host/agent.prev.sh"
t  "rollback: auto-update holds the version it fled" test -s "$SB/host/rollback-hold"
rm -f "$SB/tools.log"; env_set PREV_API_IMAGE prev3; TOOLS_FAIL=1 rb 0; rc=$?
t  "rollback: images back but the tools not exits 5" eq "$rc:$(grep -c '^images' "$SB/tools.log")" 5:1
rm -f "$SB/host/install.prev.sh" "$SB/tools.log" "$SB/host/rollback-hold"; env_set PREV_API_IMAGE prev2; NO_MANIFEST=1 rb 0; rc=$?
t  "rollback with no earlier installer still rolls back" eq "$rc:$(grep -c '^images' "$SB/tools.log")" 0:1
nt "...and leaves the tools"                        grep -q '^tools' "$SB/tools.log"
nt "...and an unreadable registry leaves no hold"   test -e "$SB/host/rollback-hold"
rm -f "$SB/tools.log"; env_set PREV_API_IMAGE ''; rb 0; rc=$?
t  "rollback with no previous version exits 6"      eq "$rc" 6
nt "...before any backup"                           test -e "$SB/tools.log"
# shellcheck disable=SC2016 # literal text on purpose
t  "the CLI is written by rename (a running dahae update keeps its file)" grep -qF 'mv -f "$tmp" "$1"' "$ROOT/deploy/install.sh"
# shellcheck disable=SC2016 # literal text on purpose
t  "the agent service may rewrite the CLI and its units" grep -qF 'ReadWritePaths=$DIR /etc/cron.d /etc/logrotate.d /run /usr/local/bin /etc/systemd/system' "$ROOT/deploy/install.sh"
( DAHAE_SELFTEST=1 . "$SB/dahae"; cd "$SB"; cmd_keys_confirm ) >/dev/null 2>&1
t  "keys-confirm records the key copy"              test -n "$(env_get KEY_ESCROW_AT)"
printf 'DB_PASSWORD=%s\nDATA_KEY=%s\nJWT_SECRET=%s\nAPI_DB_PASSWORD=%s\n' "$(printf 'a%.0s' $(seq 48))" "$(printf 'b%.0s' $(seq 64))" "$(printf 'c%.0s' $(seq 64))" "$(printf 'd%.0s' $(seq 48))" > "$SB/.env"
mkdir -p "$SB/backups"; rm -f "$SB"/backups/keys-*.env.enc
( DAHAE_SELFTEST=1 . "$SB/dahae"; cd "$SB"; has_tty() { return 1; }; DAHAE_KEYS_PASSPHRASE='correct horse battery' cmd_keys_backup ) >/dev/null 2>&1
# shellcheck disable=SC2012 # test file names are known and plain
copy=$(ls "$SB"/backups/keys-*.env.enc 2>/dev/null | head -n1)
t  "keys-backup seals exactly the two keys keys-restore reads" eq "$(keys_open 'correct horse battery' < "$copy")" "$(printf 'DATA_KEY=%s\nJWT_SECRET=%s' "$(printf 'b%.0s' $(seq 64))" "$(printf 'c%.0s' $(seq 64))")"

# ---- install.sh tools mode (what dahae update and rollback run): only the tools are rewritten, the server stays ----
tools_main() { # tools_main : 설치된 서버(가짜)에서 MALKKEUM_HOST_UPDATE=tools 로 main 을 돌려요. 단계는 main.log 에
  ( DAHAE_SELFTEST=1 . "$ROOT/deploy/install.sh"; set +e
    DIR=$SB/srv BIN=$SB/srv/bin; mkdir -p "$BIN"
    log() { printf '%s\n' "$*" >> "$SB/main.log"; }
    id() { echo 0; }
    for f in preflight install_packages dns_check harden_host write_fresh_env final_banner; do eval "$f() { log 'BAD $f'; }"; done
    install_cosign() { log cosign; }; write_schedules() { log schedules; }; install_agent() { log "install_agent ready=${AGENT_READY:-0}"; }
    dc() { log "dc $*"; return 0; }
    PATH="$SB/fakebin:$PATH" # a dahae update or prepare would reach docker: the fake one logs it
    MALKKEUM_HOST_UPDATE=tools MALKKEUM_AGENT_FILE=$SB/agent-src.sh main ) >/dev/null 2>&1
}
mkdir -p "$SB/srv" "$SB/fakebin"; rm -f "$SB/main.log"
printf '#!/usr/bin/env bash\necho "docker $*" >> "%s/main.log"\n' "$SB" > "$SB/fakebin/docker"; chmod +x "$SB/fakebin/docker"
printf '#!/usr/bin/env bash\necho agent\n' > "$SB/agent-src.sh"
printf 'DOMAIN=myshop.co.kr\nACME_EMAIL=owner@myshop.co.kr\nCHANNEL=stable\nVERSION=stable\nAPI_IMAGE=repo/api@sha256:aa\nWEB_IMAGE=repo/web@sha256:bb\nSIGNER_REPO=EricWijesinghe/Dahae\nAPI_DB_PASSWORD=%s\nDATA_KEY=%s\nJWT_SECRET=%s\n' \
  "$(printf 'd%.0s' $(seq 48))" "$(printf 'b%.0s' $(seq 64))" "$(printf 'c%.0s' $(seq 64))" > "$SB/srv/.env"
before=$(grep -E '^(DATA_KEY|JWT_SECRET|API_IMAGE)=' "$SB/srv/.env")
tools_main; rc=$?
t  "tools mode: ok"                                 eq "$rc" 0
t  "tools mode: restarts with the new compose file"  grep -qxF 'dc up -d --remove-orphans' "$SB/main.log"
nt "tools mode: no packages, DNS, hardening or fresh keys" grep -q '^BAD' "$SB/main.log"
nt "tools mode: no image update or prepare"         grep -q '^docker ' "$SB/main.log"
t  "tools mode: the agent is the handed file, fetched before the rest" eq "$(bash "$SB/srv/agent.sh"):$(grep -c 'install_agent ready=1' "$SB/main.log")" "agent:1"
t  "tools mode: the CLI is rewritten"               test -x "$SB/srv/bin/dahae"
t  "tools mode: keys and images unchanged"          eq "$(grep -E '^(DATA_KEY|JWT_SECRET|API_IMAGE)=' "$SB/srv/.env")" "$before"
sed -i '/^API_IMAGE=/d' "$SB/srv/.env"; rm -f "$SB/main.log"; tools_main; rc=$?
nt "tools mode on a server with no images refuses"  eq "$rc" 0
nt "...before writing anything"                     grep -q '^dc ' "$SB/main.log"
rm -rf "$SB/srv" "$SB/fakebin" "$SB/main.log" "$SB/agent-src.sh"

# ---- AWS backup bucket from the template: checked, then written to .env for the app ----
s3env() { ( MALKKEUM_BACKUP_S3_BUCKET=$1 MALKKEUM_BACKUP_S3_REGION=$2; check_backup_s3_env ) >/dev/null 2>&1; }
t  "s3 env: none is fine"                           s3env "" ""
t  "s3 env: a bucket and a region"                  s3env dahae-main-backupbucket-1a2b3c ap-northeast-2
nt "s3 env: a bucket needs its region"              s3env dahae-main-backupbucket-1a2b3c ""
nt "s3 env: upper case is not a bucket"             s3env Dahae-Bucket ap-northeast-2
nt "s3 env: shell characters refused"               s3env 'a;rm -rf' ap-northeast-2
nt "s3 env: odd region refused"                     s3env dahae-bucket 'seoul'
printf 'DOMAIN=x.co.kr\n' > "$SB/.env"
# shellcheck disable=SC2034 # read by apply_backup_s3_env
( MALKKEUM_BACKUP_S3_BUCKET=dahae-main-backupbucket-1a2b3c MALKKEUM_BACKUP_S3_REGION=ap-northeast-2; apply_backup_s3_env )
t  "s3 env: written to .env"                        eq "$(env_get BACKUP_S3_BUCKET) $(env_get BACKUP_S3_REGION)" "dahae-main-backupbucket-1a2b3c ap-northeast-2"
( unset MALKKEUM_BACKUP_S3_BUCKET; apply_backup_s3_env )
t  "s3 env: a re-run without it keeps the bucket"   eq "$(env_get BACKUP_S3_BUCKET)" dahae-main-backupbucket-1a2b3c
# shellcheck disable=SC2016 # literal ${...} on purpose
t  "compose hands the bucket to the api"            grep -qF 'BACKUP_S3_BUCKET: ${BACKUP_S3_BUCKET:-}' "$ROOT/deploy/docker-compose.yml"
# shellcheck disable=SC2016 # literal ${...} searched in the template
t  "the template passes the bucket and region to the installer" grep -qF 'MALKKEUM_BACKUP_S3_BUCKET=${BackupBucket} MALKKEUM_BACKUP_S3_REGION=${AWS::Region}' "$ROOT/deploy/aws/dahae.cfn.yaml"
nt "tools mode needs an installed server"           installed_here
printf 'API_IMAGE=repo/api@sha256:aa\n' >> "$SB/.env"
t  "...which has its images set"                    installed_here
rm -rf "$SB/.env" "$SB/host" "$SB/rel" "$SB/tools.log"

bash "$ROOT/scripts/ci/agent-selftest.sh" >/dev/null ||{ FAIL=$((FAIL + 1)); echo "FAIL: agent self-test (run scripts/ci/agent-selftest.sh)"; }

echo "installer self-test: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]
