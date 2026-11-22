#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Writes per-build native key files from CI env vars (the store environment in store-apps.yml). Never commit outputs.
#   scripts/ci/native-keys.sh customer|partner
# APP_LINK_DOMAIN = the server DOMAIN (Naver https App Link host), or a comma list after a domain move: the current
#   domain first, then former ones (three at most). The app signs in through the first; every host is an App Link.
#   Optional: no host, no Naver button.
# APP_LABEL, APP_ID = the store name and app id (scripts/store_brand.py). Unset: the built-in values in Flutter/*.xcconfig.
# Outputs:
#   app/apps/<app>/keys.json                  -> flutter build ... --dart-define-from-file=keys.json
#   app/apps/<app>/ios/Flutter/Keys.xcconfig  -> URL schemes / GIDClientID (Info.plist reads these)
#   android/app/google-services.json, ios/Runner/GoogleService-Info.plist -> Firebase push (optional)
# Every key is optional: a missing key hides that sign-in button or disables push, the build still passes.
set -euo pipefail
app="${1:?customer or partner}"
case "$app" in customer|partner) ;; *) echo "unknown app: $app" >&2; exit 2 ;; esac
root="$(cd "$(dirname "$0")/../.." && pwd)/app/apps/$app"
up=$(printf '%s' "$app" | tr '[:lower:]' '[:upper:]')

# Partner Firebase files have their own vars (separate Firebase apps); everything else is shared.
pick() { local own="${up}_$1"; printf '%s' "${!own:-${!1:-}}"; }

# A Google iOS OAuth client belongs to one bundle id, so the partner app takes only its own: with the customer's id
# its Google button would fail on iOS. App Review signs in to the partner app with Google (docs/store/review-notes.md).
if [ "$app" = partner ]; then
  export GOOGLE_IOS_CLIENT_ID="${PARTNER_GOOGLE_IOS_CLIENT_ID:-}"
  wanted=",$(printf '%s' "${SOCIAL_PROVIDERS:-}" | tr -d ' '),"
  if [ -z "${GOOGLE_SERVER_CLIENT_ID:-}" ] || { [ "$wanted" != ",," ] && [[ "$wanted" != *,google,* ]]; }; then
    echo "::warning::partner build without Google sign-in (GOOGLE_SERVER_CLIENT_ID unset or SOCIAL_PROVIDERS without google): App Review cannot sign in"
  elif [ -z "$GOOGLE_IOS_CLIENT_ID" ] && [ "${RUNNER_OS:-}" = macOS ]; then
    echo "::warning::partner iOS build without Google sign-in: set PARTNER_GOOGLE_IOS_CLIENT_ID (the iOS OAuth client of the partner bundle id)"
  fi
fi

# The dart-define gets the current domain only; the native App Link entries get every host.
hosts=$(printf '%s' "${APP_LINK_DOMAIN:-}" | tr -d ' ')
IFS=, read -r host1 host2 host3 extra <<< "$hosts" || true
[ -z "${extra:-}" ] || { echo "APP_LINK_DOMAIN lists more than three hosts" >&2; exit 2; }
for h in "${host2:-}" "${host3:-}"; do
  [ -z "$h" ] || printf '%s' "$h" | grep -Eq '^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$' || { echo "APP_LINK_DOMAIN: bad host $h" >&2; exit 2; }
done
export APP_LINK_DOMAIN="${host1:-}"

python3 -I - "$root/keys.json" <<'PY'
import json, os, re, sys
names = ["API_BASE", "KAKAO_NATIVE_KEY", "NAVER_CLIENT_ID", "NAVER_CLIENT_SECRET", "NAVER_CLIENT_NAME",
         "GOOGLE_SERVER_CLIENT_ID", "GOOGLE_IOS_CLIENT_ID", "SOCIAL_PROVIDERS", "APP_LINK_DOMAIN"]
keys = {n: os.environ[n] for n in names if os.environ.get(n)}
if "API_BASE" not in keys or not keys["API_BASE"].startswith("https://"):
    sys.exit("API_BASE must be set to https://<client domain>")
# Host only (the server DOMAIN): it goes into the App Link manifest entry and the entitlement as-is.
if "APP_LINK_DOMAIN" in keys and not re.fullmatch(r"[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?", keys["APP_LINK_DOMAIN"]):
    sys.exit("APP_LINK_DOMAIN must be a bare host name, e.g. malkkeum.co.kr")
json.dump(keys, open(sys.argv[1], "w"), indent=1)
print("dart-defines:", ", ".join(sorted(keys)))
PY

gid="${GOOGLE_IOS_CLIENT_ID:-}"
reversed="google-unset"
[ -n "$gid" ] && reversed="com.googleusercontent.apps.${gid%.apps.googleusercontent.com}"
cat > "$root/ios/Flutter/Keys.xcconfig" <<EOF
KAKAO_URL_SCHEME = kakao${KAKAO_NATIVE_KEY:-unset}
GOOGLE_IOS_CLIENT_ID = ${gid:-google-unset}
GOOGLE_REVERSED_CLIENT_ID = $reversed
APP_LINK_HOST = ${APP_LINK_DOMAIN:-app-link-unset}
APP_LINK_HOST_2 = ${host2:-${APP_LINK_DOMAIN:-app-link-unset}}
APP_LINK_HOST_3 = ${host3:-${APP_LINK_DOMAIN:-app-link-unset}}
EOF
# Store name and bundle id only when the store build set them; otherwise Flutter/*.xcconfig keeps the built-in ones.
if [ -n "${APP_LABEL:-}" ]; then printf 'APP_DISPLAY_NAME = %s\n' "$APP_LABEL" >> "$root/ios/Flutter/Keys.xcconfig"; fi
if [ -n "${APP_ID:-}" ]; then printf 'APP_BUNDLE_ID = %s\n' "$APP_ID" >> "$root/ios/Flutter/Keys.xcconfig"; fi

json=$(pick GOOGLE_SERVICES_JSON_B64)
plist=$(pick GOOGLE_SERVICE_INFO_PLIST_B64)
if [ -n "$json" ]; then printf '%s' "$json" | base64 -d > "$root/android/app/google-services.json"; echo "firebase android: on"; else echo "firebase android: off"; fi
if [ -n "$plist" ]; then printf '%s' "$plist" | base64 -d > "$root/ios/Runner/GoogleService-Info.plist"; echo "firebase ios: on"; else echo "firebase ios: off"; fi
