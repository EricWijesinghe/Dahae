#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# End-to-end install test. Runs the real deploy/install.sh and the `dahae` command it installs against the two
# server images built on this machine, then uses the stack the way an owner would.
#
# Two image sources:
#   registry (default)       images pushed to a local registry on :5000 (.github/workflows/e2e.yml)
#   E2E_IMAGES_TAR=<f.tar>   `docker save` of api + web (+ postgres:17), installed offline (scripts/e2e/wsl-e2e.sh)
# Step 8b switches to a test edition and back (deploy/editions mechanism). Other knobs: E2E_LOG_DIR (default: a temp dir), E2E_DOMAIN (default dahae.test), E2E_NEW_DOMAIN (the domain-move
# target, default dahae2.test; written to /etc/hosts for the run and removed at the end).
# Needs Docker (or lets install.sh install it), root or sudo, python3 and curl. Leaves nothing behind: the last
# step runs `dahae uninstall --purge`.
set -euo pipefail
cd "$(dirname "$0")/.."

TAG=e2e
DOMAIN=${E2E_DOMAIN:-dahae.test}
NEW_DOMAIN=${E2E_NEW_DOMAIN:-dahae2.test}
TAR=${E2E_IMAGES_TAR:-}
LOG=${E2E_LOG_DIR:-$(mktemp -d)}
SUDO=; [ "$(id -u)" = 0 ] || SUDO=sudo
OWNER_EMAIL=owner@example.com
OWNER_PASSWORD=e2e-owner-password
FINGERPRINT=$(printf 'AB:%.0s' $(seq 31))AB
TEAM_ID=ABCDE12345
HEARTBEAT=https://example.com/e2e-heartbeat
PROJECT=malkkeum # compose project = basename of /opt/malkkeum
if [ -n "$TAR" ]; then
  REG=ghcr.io/ericwijesinghe
  ENVS=("MALKKEUM_IMAGES_TAR=$TAR")
else
  REG=localhost:5000
  ENVS=("MALKKEUM_IMAGES=$REG" "MALKKEUM_SKIP_SIGNATURE=1") # locally built images carry no release signature
fi
ENVS+=("MALKKEUM_DOMAIN=$DOMAIN" "MALKKEUM_EMAIL=ci@example.com" "MALKKEUM_VERSION=$TAG" "MALKKEUM_TLS=internal"
  "MALKKEUM_HEALTH_WAIT=240" "MALKKEUM_AGENT_FILE=$(pwd)/deploy/agent.sh" "MALKKEUM_FIREWALL=on"
  "DAHAE_LANG=ko") # the steps below read the Korean texts; step 9c checks English and Arabic
mkdir -p "$LOG"

step() { printf '\n== %s  (%s)\n' "$*" "$(date +%T)"; }
fail() {
  printf '::error::%s\n' "$*"
  (cd /opt/malkkeum && $SUDO docker compose --env-file .env ps && $SUDO docker compose --env-file .env logs --tail 80) || true
  exit 1
}
expect() { # expect "설명" 명령... : 명령이 성공해야 통과
  local what="$1"; shift
  "$@" || fail "$what"
  echo "ok: $what"
}
curl_d() { curl -sk --max-time 20 --resolve "$DOMAIN:443:127.0.0.1" "$@"; }
get() { curl_d "https://$DOMAIN$1"; }
code() { curl_d -o /dev/null -w '%{http_code}' "https://$DOMAIN$1"; }
post() { curl_d -H 'Content-Type: application/json' -X POST "https://$DOMAIN$1" -d "$2"; }
get_auth() { curl_d -H "Authorization: Bearer $2" "https://$DOMAIN$1"; }
dc() { (cd /opt/malkkeum && $SUDO docker compose --env-file .env "$@"); }
dbq() { dc exec -T db psql -U app -d cleaning -tAc "$1"; }
dahae() { $SUDO /usr/local/bin/dahae "$@"; }
install_once() { $SUDO env "${ENVS[@]}" bash deploy/install.sh; }
publish() { [ -n "$TAR" ] || docker push -q "$1" >/dev/null; } # tar mode: the stack uses this engine's images
json() { python3 -I -c 'import json, sys
v = json.load(sys.stdin)
for k in sys.argv[1:]: v = v[k]
print(v if not isinstance(v, (dict, list)) else json.dumps(v))' "$@"; }
totp() { python3 -I -c 'import base64, hashlib, hmac, struct, sys, time
s = sys.argv[1]; key = base64.b32decode(s + "=" * (-len(s) % 8))
h = hmac.new(key, struct.pack(">Q", int(time.time()) // 30), hashlib.sha1).digest(); o = h[-1] & 15
print("%06d" % ((struct.unpack(">I", h[o:o + 4])[0] & 0x7FFFFFFF) % 1000000))' "$1"; }
fresh_totp() { # each 30 s step is accepted once: wait for one not used yet (a file, as callers run in $(...))
  while [ "$(($(date +%s) / 30))" = "$(cat "$LOG/totp-step" 2>/dev/null)" ]; do sleep 2; done
  echo "$(($(date +%s) / 30))" > "$LOG/totp-step"; totp "$SECRET"
}
memcap() { # the stack's own caps (db 768m + api 512m + web 384m = 1664m, fits a 2 GB VPS); compose auto-loads the override
  printf 'services:\n  db: {mem_limit: 768m}\n  api: {mem_limit: 512m}\n  web: {mem_limit: 384m}\n' \
    | $SUDO tee /opt/malkkeum/docker-compose.override.yml >/dev/null
}
wait_until() { # wait_until 초 명령... : 명령이 성공할 때까지 기다려요
  local limit="$1" waited=0; shift
  until "$@" >/dev/null 2>&1; do [ "$waited" -ge "$limit" ] && return 1; sleep 5; waited=$((waited + 5)); done
}

api_healthy() { get /health | grep -q '"ok":true'; }
api_version_is_test() { get /health | grep -q "\"version\":\"$TAG\""; }
website_served() { [ "$(code /)" = 200 ] && get / | grep -qi '<html'; }
# Every page's fragments reach the API from Caddy's own address (about 22 per home page view, 300 a minute for one
# address): a busy minute must not turn the site into the maintenance page.
pages_survive_a_burst() { for _ in $(seq 40); do [ "$(code /)" = 200 ] || return 1; done; }
# The domain root approved, which the apps check before following a new api_base.
approved_domain_served() { [ "$(get /.well-known/dahae-domain)" = "$DOMAIN" ]; }
# API down: a proxied non-API path gets the static maintenance page (Caddy's 502 handler), then the API comes back.
maintenance_page_when_api_down() {
  dc stop api >/dev/null 2>&1 || return 1
  local shown=1
  case "$(code /)" in 502|503) get / | grep -q '점검 중이에요' && shown=0 ;; esac
  [ "$(code /health)" = 200 ] && shown=1 # a dead API must not look healthy to probes
  dc up -d api >/dev/null 2>&1
  wait_until 180 api_healthy || return 1
  return $shown
}
console_served() { [ "$(code /admin/)" = 200 ] && get /admin/ | grep -q 'base href="/admin/"'; }
csp_of() { curl_d -D - -o /dev/null "https://$DOMAIN$1" | grep -i '^content-security-policy:'; }
csp_tight() { # apps: connect-src only to picked files (blob:) and the payment gateways (PortOne, Toss), no unsafe-eval, nothing may frame us
  local csp; csp="$(csp_of /admin/)"
  echo "$csp" | grep -q "connect-src 'self' blob: https://\*\.portone\.io https://\*\.tosspayments\.com;" \
    && echo "$csp" | grep -q "frame-ancestors 'none'" \
    && ! echo "$csp" | grep -qi "'unsafe-eval'" # 'wasm-unsafe-eval' (Flutter --wasm) is fine
}
site_csp_strict() { # website: inline scripts by hash only, nothing but same-origin calls
  local csp; csp="$(csp_of /)"
  echo "$csp" | grep -q "script-src 'self' 'sha256-" && ! echo "$csp" | grep -q "'unsafe-inline' 'wasm" \
    && echo "$csp" | grep -q "connect-src 'self';" && echo "$csp" | grep -q "frame-ancestors 'none'"
}
hardened() { # hardened 서비스 : read-only root, every capability dropped, no privilege gain
  local id; id=$(dc ps -q "$1") && [ -n "$id" ] \
    && [ "$($SUDO docker inspect -f '{{.HostConfig.ReadonlyRootfs}} {{.HostConfig.CapDrop}} {{.HostConfig.SecurityOpt}}' "$id")" = "true [ALL] [no-new-privileges:true]" ]
}
KEYS_PASS=e2e-keys-passphrase
keys_file() { $SUDO sh -c 'ls -t /opt/malkkeum/backups/keys-*.env.enc 2>/dev/null | head -n1'; }
data_key() { $SUDO grep -E '^DATA_KEY=' /opt/malkkeum/.env | cut -d= -f2; }
needs_owner() { get /v1/setup | grep -q '"needs_admin":true'; }
owner_is_set_up() { get /v1/setup | grep -q '"needs_admin":false'; }
staff_count_is() { [ "$(dbq 'select count(*) from staff')" = "$1" ]; }
setting_count_is() { [ "$(dbq "select count(*) from settings where key = '$1'")" = "$2" ]; }
setup_code_is() { [ "$(dahae setup-code)" = "$1" ]; }
app_role_ok() { # the API signs in as dahae_api, which is not a superuser
  [ "$(dbq "select rolsuper from pg_roles where rolname = 'dahae_api'")" = f ]     && [ "$(dbq "select count(*) > 0 from pg_stat_activity where usename = 'dahae_api'")" = t ]     && [ "$(dbq "select pg_get_userbyid(datdba) from pg_database where datname = 'cleaning'")" = dahae_api ]
}
purge_needs_domain() { ! $SUDO setsid -w env DAHAE_YES=1 /usr/local/bin/dahae uninstall --purge --yes </dev/null >/dev/null 2>&1; } # no terminal
mode_is() { [ "$($SUDO stat -c %a "$1")" = "$2" ]; }
version_shown() { dahae version | grep "$TAG" >/dev/null; } # grep -q would close the pipe while dahae still writes
backups_count() { $SUDO sh -c 'ls /opt/malkkeum/backups/db-*.sql.gz.enc 2>/dev/null | wc -l'; }
plaintext_dumps_count() { $SUDO sh -c 'ls /opt/malkkeum/backups/db-*.sql.gz 2>/dev/null | wc -l'; }
create_owner() {
  post /v1/setup/admin "{\"code\":\"$CODE\",\"email\":\"$OWNER_EMAIL\",\"name\":\"Owner\",\"password\":\"$OWNER_PASSWORD\"}" \
    | grep -q '"ok":true'
}
staff_login() { post /v1/auth/staff/login "{\"email\":\"$OWNER_EMAIL\",\"password\":\"$OWNER_PASSWORD\"}"; }
owner_token() { # console sign-in: password, then the TOTP second factor
  local mfa; mfa=$(staff_login | json mfa_token)
  post /v1/auth/staff/mfa/verify "{\"mfa_token\":\"$mfa\",\"code\":\"$(fresh_totp)\"}" | json access_token
}
wizard_has_sections() { local o; o=$(get_auth /v1/admin/setup "$TOKEN"); for s in portone tosspayments solapi backup business; do echo "$o" | grep -q "\"$s\"" || return 1; done; }
heartbeat_on_in_wizard() { get_auth /v1/admin/setup "$(owner_token)" | grep -q '"heartbeat_configured":true'; }
upload_marker() { dc exec -T api cat /data/uploads/e2e-marker.txt 2>/dev/null | grep -q e2e-upload; }
no_old_brand() { ! grep -q '말끔' "$LOG/install1.txt"; }
rolled_back_to_pinned() { $SUDO grep -Eq '^API_IMAGE=.*(@sha256:|:keep-)' /opt/malkkeum/.env; }
api_env_is() { [ "$(dc exec -T api printenv "$1" 2>/dev/null)" = "$2" ]; }
# A former domain after `dahae domain`: curl to it on this machine, and what the API recorded.
old_curl() { curl -sk --max-time 20 --resolve "$OLD_DOMAIN:443:127.0.0.1" "$@"; }
old_code() { old_curl -o /dev/null -w '%{http_code}' "https://$OLD_DOMAIN$1"; }
old_alias_recorded() { [ "$(dbq "select value->'aliases'->0->>'host' from settings where key = 'domains'")" = "$OLD_DOMAIN" ]; }
old_api_served() { [ "$(old_code /v1/config)" = 200 ]; }
old_names_new_domain() { [ "$(old_curl "https://$OLD_DOMAIN/.well-known/dahae-domain")" = "$NEW_DOMAIN" ]; }
old_app_links_served() { old_curl "https://$OLD_DOMAIN/.well-known/assetlinks.json" | grep -q "$FINGERPRINT"; }
old_page_moved() { [ "$(old_curl -o /dev/null -w '%{http_code} %{redirect_url}' "https://$OLD_DOMAIN/")" = "301 https://$NEW_DOMAIN/" ]; }
stranger_refused() { ! curl -sk --max-time 20 --resolve "stranger.test:443:127.0.0.1" -o /dev/null https://stranger.test/health; }
bad_domain_refused() { ! $SUDO env DAHAE_YES=1 /usr/local/bin/dahae domain localhost </dev/null >/dev/null 2>&1; }
drop_hosts_line() { $SUDO sed -i '/ # dahae-e2e$/d' /etc/hosts; }
put_auth() { curl_d -H "Authorization: Bearer $2" -H 'Content-Type: application/json' -X PUT "https://$DOMAIN$1" -d "$3"; }
# Step 9c (R4): the wizard of a fresh install, whose country is still unset, in English and in Arabic.
no_country_yet() { [ "$(get_auth /v1/admin/setup/packs "$W" | json current country)" = None ]; }
kr_pack_offered() { [ "$(get_auth /v1/admin/setup/packs "$W" | json default)" = KR ]; }
no_dark_adapter_offered() { ! get_auth /v1/admin/setup/packs "$W" | grep -Eq '"(stripe|twilio|google_places|oidc)"'; }
country_chosen() { [ "$(put_auth /v1/admin/setup/country "$W" '{"country":"KR"}' | json current country)" = KR ]; }
gates_enforced() { [ "$(get_auth /v1/admin/setup/gates "$W" | json enforced)" = True ]; }
site_preview_matches_gates() { [ "$(get /v1/setup | json preview)" = "$(get_auth /v1/admin/setup/gates "$W" | json preview)" ]; }
languages_set() { # languages_set ar : the console, customer and own language all become it
  local b='{"console_locale":"'"$1"'","locales":["'"$1"'"],"my_locale":"'"$1"'"}'
  [ "$(put_auth /v1/admin/setup/languages "$W" "$b" | json console_locale)" = "$1" ] \
    && [ "$(get_auth /v1/admin/setup/languages "$W" | json my_locale)" = "$1" ]
}
brand_lists_language() { [ "$(get /v1/brand | json locales)" = '["'"$1"'"]' ]; }
cli_speaks_without_korean() { # DAHAE_LANG=en|ar: the dahae command's usage text, no Korean left (ar reads English)
  $SUDO env DAHAE_LANG="$1" /usr/local/bin/dahae help | python3 -I -c 'import re, sys
t = sys.stdin.read(); sys.exit(0 if "dahae" in t and not re.search("[가-힣]", t) else 1)'
}
no_dev_code() { ! post /v1/auth/otp '{"phone":"01012345678"}' | grep -q dev_code; }
dev_api_up() { curl -fs --max-time 5 http://127.0.0.1:18080/health; }
nothing_left() {
  [ -z "$($SUDO docker ps -aq --filter "label=com.docker.compose.project=$PROJECT")" ] \
    && [ -z "$($SUDO docker volume ls -q --filter "label=com.docker.compose.project=$PROJECT")" ] \
    && ! $SUDO test -e /etc/cron.d/malkkeum && ! $SUDO test -e /etc/logrotate.d/dahae \
    && ! test -e /usr/local/bin/dahae && ! test -e /usr/local/bin/malkkeum && ! $SUDO test -e /opt/malkkeum
}

step "1. first install with the real installer ($([ -n "$TAR" ] && echo "offline tar" || echo registry))"
install_once 2>&1 | tee "$LOG/install1.txt"
[ "${PIPESTATUS[0]}" = 0 ] || fail "first install failed"
expect "installer finished" grep -q '설치가 끝났어요' "$LOG/install1.txt"
expect "installer text says 다해" grep -q '다해' "$LOG/install1.txt"
expect "no old brand in installer text" no_old_brand
expect "dahae command installed" test -x /usr/local/bin/dahae
expect "malkkeum alias kept" test -x /usr/local/bin/malkkeum
CODE=$(dahae setup-code)
expect "setup code printed by installer" grep -q "$CODE" "$LOG/install1.txt"
expect ".env readable by root only" mode_is /opt/malkkeum/.env 600
expect "backups readable by root only" mode_is /opt/malkkeum/backups 700
expect "cron jobs installed" $SUDO test -f /etc/cron.d/malkkeum
expect "log rotation installed" $SUDO test -f /etc/logrotate.d/dahae
expect "API uses the non-superuser DB role" app_role_ok
for s in db api web; do expect "$s container hardened (read-only, no capabilities)" hardened "$s"; done
expect "cron reports a failed unattended update" $SUDO grep -q 'dahae auto-update .*|| .*dahae alert update' /etc/cron.d/malkkeum
expect "ufw active" sh -c "$SUDO ufw status | grep -q 'Status: active'"
expect "ufw allows 22, 80 and 443" sh -c "$SUDO ufw status | grep -qE '^22/tcp +ALLOW' && $SUDO ufw status | grep -qE '^80/tcp +ALLOW' && $SUDO ufw status | grep -qE '^443/tcp +ALLOW'"
expect "fail2ban running with the sshd jail" $SUDO fail2ban-client status sshd
expect "unattended security upgrades on" $SUDO grep -q 'Unattended-Upgrade "1"' /etc/apt/apt.conf.d/20auto-upgrades
expect "agent path unit is active" $SUDO systemctl is-active --quiet dahae-agent.path
expect "agent timer is active" $SUDO systemctl is-active --quiet dahae-agent.timer
expect "job status folder is root-owned" sh -c "[ \"\$($SUDO stat -c %u /opt/malkkeum/jobs/status)\" = 0 ]"
expect "job inbox belongs to the API user" sh -c "[ \"\$($SUDO stat -c %u /opt/malkkeum/jobs/inbox)\" = 10001 ]"
memcap; dc up -d >/dev/null 2>&1
expect "healthy with 2 GB VPS memory limits" wait_until 180 api_healthy

step "2. services answer over HTTPS"
expect "API health" api_healthy
expect "API reports the image version" api_version_is_test
expect "website served" website_served
expect "40 home page views in a row all answer 200" pages_survive_a_burst
expect "the approved domain is served for the apps" approved_domain_served
expect "console served at /admin/" console_served
expect "app CSP limits connect-src to the payment gateways and forbids eval" csp_tight
expect "website CSP allows inline scripts by hash only" site_csp_strict
expect "setup wizard waits for the owner" needs_owner
expect "dahae status" dahae status
expect "dahae version shows the image version" version_shown
expect "API down: a page path shows the maintenance page" maintenance_page_when_api_down

step "3. owner account, console sign-in, setup wizard"
expect "owner created with the setup code" create_owner
expect "one staff row" staff_count_is 1
expect "setup code is single use" test "$(post /v1/setup/admin "{\"code\":\"$CODE\",\"email\":\"x@example.com\",\"name\":\"X\",\"password\":\"another-password-1\"}" | grep -c '"ok":true')" = 0
MFA=$(staff_login | json mfa_token)
expect "first console sign-in asks for TOTP enrolment" test -n "$MFA"
SECRET=$(post /v1/auth/staff/mfa/enrol "{\"mfa_token\":\"$MFA\"}" | json secret)
TOKEN=$(post /v1/auth/staff/mfa/enrol/confirm "{\"mfa_token\":\"$MFA\",\"code\":\"$(fresh_totp)\"}" | json access_token)
expect "TOTP enrolment signs the owner in" test -n "$TOKEN"
expect "wizard lists payments, SMS, backup, business" wizard_has_sections
expect "second sign-in with TOTP" test -n "$(owner_token)"

step "4. backup and restore round trip"
dbq "insert into settings (key, value) values ('e2e_before', 'true')" >/dev/null
dc exec -T api sh -c 'echo e2e-upload > /data/uploads/e2e-marker.txt'
expect "dahae backup" dahae backup
BACKUP=$($SUDO sh -c 'ls -t /opt/malkkeum/backups/db-*.sql.gz.enc | head -n1')
expect "local dump is sealed (MKB1)" sh -c "$SUDO head -c 4 '$BACKUP' | grep -q '^MKB1'"
expect "no plaintext dump left behind" test "$(plaintext_dumps_count)" = 0
dbq "insert into settings (key, value) values ('e2e_after', 'true')" >/dev/null
dc exec -T api rm /data/uploads/e2e-marker.txt
dahae restore "$BACKUP" --yes 2>&1 | tee "$LOG/restore.txt"
[ "${PIPESTATUS[0]}" = 0 ] || fail "dahae restore failed"
expect "restore says done" grep -q '복원 완료' "$LOG/restore.txt"
expect "restore leaves no opened plaintext behind" test "$(plaintext_dumps_count)" = 0
expect "row from before the backup is back" setting_count_is e2e_before 1
expect "row written after the backup is gone" setting_count_is e2e_after 0
expect "uploads restored" upload_marker
expect "owner kept" staff_count_is 1
expect "API healthy after restore" api_healthy
expect "restore kept the DB owned by the app role" app_role_ok
KEY_BEFORE=$(data_key)
expect "dahae keys-backup" $SUDO env DAHAE_KEYS_PASSPHRASE=$KEYS_PASS /usr/local/bin/dahae keys-backup
expect "key copy is not plain text" sh -c "! $SUDO grep -q DATA_KEY '$(keys_file)'"
expect "wrong passphrase changes nothing" sh -c "! $SUDO env DAHAE_KEYS_PASSPHRASE=wrong-passphrase-x /usr/local/bin/dahae keys-restore '$(keys_file)' --yes"
expect "dahae keys-restore" $SUDO env DAHAE_KEYS_PASSPHRASE=$KEYS_PASS /usr/local/bin/dahae keys-restore "$(keys_file)" --yes
expect "same DATA_KEY after the round trip" test "$(data_key)" = "$KEY_BEFORE"
expect "API healthy after keys-restore" api_healthy

step "5. re-running install.sh is a safe upgrade"
before=$(backups_count)
install_once > "$LOG/install2.txt" 2>&1 || { tail -n 40 "$LOG/install2.txt"; fail "second install failed"; }
expect "installer detected the existing install" grep -q '기존 설치를 찾았어요' "$LOG/install2.txt"
expect "re-run backed up first" test "$(backups_count)" -gt "$before"
expect "owner row survived" staff_count_is 1
expect "setup code unchanged" setup_code_is "$CODE"
expect "API still healthy" api_healthy
expect "owner still set up" owner_is_set_up
expect "banner does not show a dead setup code" sh -c "! grep -q '$CODE' '$LOG/install2.txt'"

# a job file as the API writes it: owned by the API user (10001), UUIDv7 named by the current time
queue_job() { # queue_job 종류 : 작업 파일을 inbox 에 넣고 번호를 돌려줘요
  local ms id
  ms=$(($(date +%s) * 1000)); id=$(printf '%012x' "$ms")
  id="${id:0:8}-${id:8:4}-7000-8000-$(printf '%012d' "$RANDOM$RANDOM")"
  printf 'id=%s\nkind=%s\n' "$id" "$1" > "$LOG/$id.job"
  $SUDO install -o 10001 -g 10001 -m 600 "$LOG/$id.job" "/opt/malkkeum/jobs/inbox/$id.job" && echo "$id"
}
job_state_is() { $SUDO grep -q "\"state\":\"$2\"" "/opt/malkkeum/jobs/status/$1.json" 2>/dev/null; }

step "5b. a queued job is run by the root agent"
JOB=$(queue_job status-refresh)
expect "agent runs the queued job" wait_until 90 job_state_is "$JOB" ok
expect "agent wrote host.json" $SUDO test -s /opt/malkkeum/host/host.json

step "6. a broken release rolls back by itself"
docker tag "$REG/malkkeum-api:$TAG" "$REG/malkkeum-api:good"
publish "$REG/malkkeum-api:good"
mkdir -p "$LOG/broken"
printf 'FROM %s/malkkeum-api:%s\nCMD ["sh", "-c", "echo broken release; exit 1"]\n' "$REG" "$TAG" \
  | docker build -q -f - -t "$REG/malkkeum-api:$TAG" "$LOG/broken" >/dev/null
publish "$REG/malkkeum-api:$TAG"
set +e
$SUDO env MALKKEUM_HEALTH_WAIT=60 /usr/local/bin/dahae update 2>&1 | tee "$LOG/update-broken.txt"
rc=${PIPESTATUS[0]}
set -e
expect "update exits non-zero" test "$rc" -ne 0
expect "update says it rolled back" grep -q '이전 버전으로 되돌렸어요' "$LOG/update-broken.txt"
expect "update exits 3 when it rolled back" test "$rc" -eq 3
JOB=$(queue_job update)
expect "a queued update that rolls back ends rolled_back" wait_until 900 job_state_is "$JOB" rolled_back
expect "API healthy after rollback" api_healthy
expect "running the pinned previous image" rolled_back_to_pinned
expect "data survived the failed update" staff_count_is 1

step "7. a good release updates normally"
docker tag "$REG/malkkeum-api:good" "$REG/malkkeum-api:$TAG"
publish "$REG/malkkeum-api:$TAG"
dahae update 2>&1 | tee "$LOG/update-good.txt"
expect "update succeeded" grep -q '업데이트했어요' "$LOG/update-good.txt"
expect "API healthy after update" api_healthy
expect "data survived the update" staff_count_is 1

step "8. heartbeat and app links without editing .env"
dahae heartbeat "$HEARTBEAT" 2>&1 | tee "$LOG/heartbeat.txt"
expect "heartbeat URL reaches the API" api_env_is HEARTBEAT_URL "$HEARTBEAT"
expect "wizard shows heartbeat on" heartbeat_on_in_wizard
expect "bad heartbeat URL refused" sh -c "! $SUDO /usr/local/bin/dahae heartbeat 'http://x.example/a b'"
expect "heartbeat off" dahae heartbeat off
expect "heartbeat URL cleared" api_env_is HEARTBEAT_URL ""
expect "dahae applinks" dahae applinks --android-sha256 "$FINGERPRINT" --apple-team-id "$TEAM_ID"
expect "assetlinks.json lists the signing cert" sh -c "curl -sk --resolve '$DOMAIN:443:127.0.0.1' https://$DOMAIN/.well-known/assetlinks.json | grep -q '$FINGERPRINT'"
expect "apple-app-site-association names the team" sh -c "curl -sk --resolve '$DOMAIN:443:127.0.0.1' https://$DOMAIN/.well-known/apple-app-site-association | grep -q '$TEAM_ID'"
expect "bad fingerprint refused" sh -c "! $SUDO /usr/local/bin/dahae applinks --android-sha256 nope"

step "8b. edition switch: a broken stream goes back with exit 3 and the old triple, a good one switches, and back again"
# A test table (DAHAE_EDITIONS_FILE, honoured only with signatures off): edtest lists this domain, so it is offered.
# Images of the edtest stream sit under $REG/edtest; each stream's channel version is a file:// delivery folder.
ED_DIST=$LOG/edtest-dist DAHAE_DIST=$LOG/dahae-dist
mkdir -p "$ED_DIST/main" "$DAHAE_DIST/main"; echo 9.9.9 > "$ED_DIST/main/VERSION"; echo 9.9.10 > "$DAHAE_DIST/main/VERSION"
printf 'dahae %s EricWijesinghe/Dahae file://%s *\nedtest %s/edtest e2e/Edtest file://%s %s\n' \
  "$REG" "$DAHAE_DIST" "$REG" "$ED_DIST" "$DOMAIN" > "$LOG/editions"
env_line() { $SUDO grep -E "^$1=" /opt/malkkeum/.env | tail -n1; }
triple() { for k in EDITION IMAGE_REPO SIGNER_REPO DELIVERY_RAW VERSION; do env_line "$k"; done; }
edtag() { # edtag 이미지 태그... : edtest 흐름에 api(받은 이미지)와 web 을 그 태그로 올려요
  local img="$1" t; shift
  for t in "$@"; do
    docker tag "$img" "$REG/edtest/malkkeum-api:$t"; publish "$REG/edtest/malkkeum-api:$t"
    docker tag "$REG/malkkeum-web:$TAG" "$REG/edtest/malkkeum-web:$t"; publish "$REG/edtest/malkkeum-web:$t"
  done
}
install_editions() { $SUDO env "${ENVS[@]}" MALKKEUM_SKIP_SIGNATURE=1 DAHAE_EDITIONS_FILE="$LOG/editions" bash deploy/install.sh; }
install_editions > "$LOG/install-editions.txt" 2>&1 || { tail -n 40 "$LOG/install-editions.txt"; fail "installer run with the test editions failed"; }
expect "the listed domain is offered the test edition" test "$(env_line EDITIONS_OFFERED)" = EDITIONS_OFFERED=edtest
expect "a server without EDITION shows edition dahae" sh -c "$SUDO /usr/local/bin/dahae version | grep -qx 'edition: dahae'"
expect "the CLI has the table" $SUDO grep -q '^edtest ' /opt/malkkeum/host/editions
BEFORE=$(triple)
printf 'FROM %s/malkkeum-api:%s\nCMD ["sh", "-c", "echo broken release; exit 1"]\n' "$REG" "$TAG" \
  | docker build -q -f - -t "$REG/malkkeum-api:broken" "$LOG/broken" >/dev/null
edtag "$REG/malkkeum-api:broken" stable e2e
set +e
$SUDO env MALKKEUM_HEALTH_WAIT=60 /usr/local/bin/dahae edition edtest > "$LOG/edition-broken.txt" 2>&1
rc=$?
set -e
[ "$rc" -eq 3 ] || { echo "rc=$rc"; tail -n 40 "$LOG/edition-broken.txt"; }
expect "a stream that does not answer exits 3" test "$rc" -eq 3
expect "...and the old triple is back" test "$(triple)" = "$BEFORE"
expect "...with no journal left" $SUDO test ! -e /opt/malkkeum/host/edition.prev
expect "API healthy after the refused switch" wait_until 120 api_healthy
edtag "$REG/malkkeum-api:good" stable e2e
dahae edition edtest > "$LOG/edition-good.txt" 2>&1 || { tail -n 40 "$LOG/edition-good.txt"; fail "dahae edition edtest failed"; }
expect "switched: the edtest triple is in .env" test "$(triple | tr '\n' ' ')" = "EDITION=edtest IMAGE_REPO=$REG/edtest SIGNER_REPO=e2e/Edtest DELIVERY_RAW=file://$ED_DIST VERSION=stable "
expect "API healthy on the new stream" wait_until 180 api_healthy
expect "running an image of the new stream" sh -c "$SUDO grep -q '^API_IMAGE=$REG/edtest/malkkeum-api' /opt/malkkeum/.env"
# The rollback's tools run uses the previous release's installer. Seed a delivered-style one: the test table baked in
# and RELEASE_RAW pointing at the Dahae delivery repo, so a tools run that took DELIVERY_RAW from RELEASE_RAW would show.
seed_prev_installer() {
  $SUDO sh -c "sed -e '/^# @editions:/r $LOG/editions' -e 's|^RELEASE_RAW=\$|RELEASE_RAW=https://raw.githubusercontent.com/EricWijesinghe/Dahae-install/v0.0.1|' deploy/install.sh > /opt/malkkeum/host/install.prev.sh \
    && install -m 600 deploy/agent.sh /opt/malkkeum/host/agent.prev.sh && chmod 600 /opt/malkkeum/host/install.prev.sh"
}
seed_prev_installer
expect "the seeded previous installer has the Dahae delivery baked" $SUDO grep -q '^RELEASE_RAW=https://raw.githubusercontent.com/EricWijesinghe/Dahae-install/' /opt/malkkeum/host/install.prev.sh
$SUDO env MALKKEUM_SKIP_SIGNATURE=1 /usr/local/bin/dahae rollback > "$LOG/edition-rollback.txt" 2>&1 || { tail -n 40 "$LOG/edition-rollback.txt"; fail "rollback after the switch failed"; }
expect "the rollback ran the previous installer's tools" grep -q '관리 도구도 전 버전으로 되돌렸어요' "$LOG/edition-rollback.txt"
expect "rollback keeps the triple on the new edition" test "$(triple | head -n4 | tr '\n' ' ')" = "EDITION=edtest IMAGE_REPO=$REG/edtest SIGNER_REPO=e2e/Edtest DELIVERY_RAW=file://$ED_DIST "
# What a rollback's tools run does: the bridge installer runs again. Even with MALKKEUM_IMAGES set it keeps the row's triple.
install_editions > "$LOG/install-editions2.txt" 2>&1 || { tail -n 40 "$LOG/install-editions2.txt"; fail "installer re-run on the new edition failed"; }
expect "an installer re-run keeps the edition's triple" test "$(triple | head -n4 | tr '\n' ' ')" = "EDITION=edtest IMAGE_REPO=$REG/edtest SIGNER_REPO=e2e/Edtest DELIVERY_RAW=file://$ED_DIST "
expect "...and offers no edition (the dahae row is never offered)" test "$(env_line EDITIONS_OFFERED)" = EDITIONS_OFFERED=
# A switch killed after .env changed: the next auto-update puts the old triple back before anything else.
SWITCHED=$(triple)
# cut_switch: the journal a real switch writes (ending in its #complete line), then a .env half-way to the dahae row
cut_switch() {
  $SUDO sh -c "{ grep -E '^(EDITION|IMAGE_REPO|SIGNER_REPO|DELIVERY_RAW|VERSION|API_IMAGE|WEB_IMAGE|PREV_API_IMAGE|PREV_WEB_IMAGE|EDITIONS_OFFERED)=' /opt/malkkeum/.env; echo '#complete'; } > /opt/malkkeum/host/edition.prev"
  $SUDO sed -i -e 's/^EDITION=.*/EDITION=dahae/' -e "s|^IMAGE_REPO=.*|IMAGE_REPO=$REG|" -e 's|^SIGNER_REPO=.*|SIGNER_REPO=EricWijesinghe/Dahae|' \
    -e "s|^DELIVERY_RAW=.*|DELIVERY_RAW=file://$DAHAE_DIST|" /opt/malkkeum/.env
}
cut_switch
$SUDO touch /opt/malkkeum/host/skip-next-update
dahae auto-update > "$LOG/edition-crash.txt" 2>&1 || true
expect "a switch killed mid-way is restored by the next auto-update" test "$(triple)" = "$SWITCHED"
expect "...and its journal is gone" $SUDO test ! -e /opt/malkkeum/host/edition.prev
# The same cut-off switch, then the owner presses rollback: the triple is settled before the previous tools run.
# The first rollback swapped install.prev.sh away (e2e mode never keeps host/install.sh), so seed it again.
seed_prev_installer
cut_switch
expect "a previous installer is there for the rollback" $SUDO test -s /opt/malkkeum/host/install.prev.sh
$SUDO env MALKKEUM_SKIP_SIGNATURE=1 /usr/local/bin/dahae rollback > "$LOG/edition-crash-rollback.txt" 2>&1 \
  || { tail -n 40 "$LOG/edition-crash-rollback.txt"; fail "rollback after a cut-off switch failed"; }
expect "rollback after a cut-off switch keeps the edition's triple" test "$(triple | head -n4 | tr '\n' ' ')" = "EDITION=edtest IMAGE_REPO=$REG/edtest SIGNER_REPO=e2e/Edtest DELIVERY_RAW=file://$ED_DIST "
expect "...and its journal is gone" $SUDO test ! -e /opt/malkkeum/host/edition.prev
expect "API healthy after the rollback" wait_until 180 api_healthy
docker tag "$REG/malkkeum-api:good" "$REG/malkkeum-api:stable"; publish "$REG/malkkeum-api:stable"
docker tag "$REG/malkkeum-web:$TAG" "$REG/malkkeum-web:stable"; publish "$REG/malkkeum-web:stable"
dahae edition dahae > "$LOG/edition-back.txt" 2>&1 || { tail -n 40 "$LOG/edition-back.txt"; fail "switching back to dahae failed"; }
expect "back on the dahae stream" test "$(triple | head -n3 | tr '\n' ' ')" = "EDITION=dahae IMAGE_REPO=$REG SIGNER_REPO=EricWijesinghe/Dahae "
expect "API healthy back on dahae" wait_until 180 api_healthy
expect "data survived the edition switches" staff_count_is 1

step "9. scripts/smoke.py against the installed stack (temporary dev override)"
$SUDO cp scripts/e2e/override.dev.yml /opt/malkkeum/docker-compose.override.yml
$SUDO install -m 755 scripts/e2e/psql-shim.sh /usr/local/bin/dahae-e2e-psql
dc up -d api >/dev/null 2>&1
expect "dev API answers on 127.0.0.1:18080" wait_until 180 dev_api_up
PSQL=/usr/local/bin/dahae-e2e-psql python3 -I scripts/smoke.py http://127.0.0.1:18080 2>&1 | tee "$LOG/smoke.txt"
[ "${PIPESTATUS[0]}" = 0 ] || fail "smoke.py failed"
memcap; $SUDO rm -f /usr/local/bin/dahae-e2e-psql
dc up -d >/dev/null 2>&1
expect "back to production config" wait_until 180 api_env_is DEV_MODE 0
expect "API healthy in production mode" wait_until 180 api_healthy
expect "no dev login codes in production mode" no_dev_code

step "9b. domain move: the new domain serves, the old one keeps answering (apps) and sends pages over"
OLD_DOMAIN=$DOMAIN
# `dahae domain` checks https://<new>/health through the resolver, so the new name must resolve on this machine.
echo "127.0.0.1 $NEW_DOMAIN # dahae-e2e" | $SUDO tee -a /etc/hosts >/dev/null
trap drop_hosts_line EXIT
dahae domain "$NEW_DOMAIN" --yes 2>&1 | tee "$LOG/domain.txt"
[ "${PIPESTATUS[0]}" = 0 ] || fail "dahae domain failed"
DOMAIN=$NEW_DOMAIN
expect "the new domain answers" wait_until 120 api_healthy
expect "the API took the new base" api_env_is PUBLIC_BASE_URL "https://$NEW_DOMAIN"
expect "the old domain is recorded as an alias" old_alias_recorded
expect "the old domain still serves the API (installed apps)" wait_until 60 old_api_served
expect "the old domain still serves app links" old_app_links_served
expect "the old domain names the new one for the apps" old_names_new_domain
expect "old website pages move to the new domain (301)" old_page_moved
expect "a stranger's host name gets no certificate" stranger_refused
expect "a bad domain is refused" bad_domain_refused

step "9c. a fresh install's wizard in en and ar: languages, country card, go-live gates, CLI texts"
W=$(owner_token)
expect "the owner is signed in" test -n "$W"
expect "no country chosen yet" no_country_yet
expect "the KR pack is offered as the default" kr_pack_offered
expect "dark adapters are never offered" no_dark_adapter_offered
expect "the owner chooses the country" country_chosen
expect "the chosen country turns the go-live gates on" gates_enforced
expect "the website's preview flag follows the gates" site_preview_matches_gates
for l in en ar; do
  expect "languages set to $l" languages_set "$l"
  expect "the website reads $l from the brand" wait_until 60 brand_lists_language "$l"
  expect "dahae speaks $l without Korean" cli_speaks_without_korean "$l"
done

step "10. uninstall"
expect "--purge is not skipped by --yes or DAHAE_YES" purge_needs_domain
expect "still installed after the refused purge" $SUDO test -f /opt/malkkeum/.env
dahae uninstall --purge --confirm "$DOMAIN" 2>&1 | tee "$LOG/uninstall.txt"
[ "${PIPESTATUS[0]}" = 0 ] || fail "dahae uninstall failed"
expect "containers, volumes, cron, CLI and /opt/malkkeum removed" nothing_left

printf '\n✔ install end-to-end test passed\n'
