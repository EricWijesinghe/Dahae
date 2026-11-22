# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Web image: Caddy serving the website (/), the customer web app (/app/), the console (/admin/) and proxying the API.
# Build context: repository root.
# Flutter web output is the same for every CPU, so this stage always runs natively on the build machine
# (no QEMU for the linux/arm64 image); only the Caddy stage below is per-platform.
# BuildKit fills BUILDPLATFORM itself; the classic builder (no buildx) needs --build-arg BUILDPLATFORM=linux/amd64.
# No default here: a default would override BuildKit's automatic value.
# Base images are pinned by digest (Dependabot bumps them).
ARG BUILDPLATFORM
FROM --platform=$BUILDPLATFORM ghcr.io/cirruslabs/flutter:stable@sha256:46691e311715845de03a3ba4753a475476936805b29431b1f00f1816981033f8 AS console
# The :stable image can lag the Flutter/Dart SDK that app/pubspec.lock needs (Dart >= 3.13), so pin the SDK here.
# Same version as CI and the store builds (scripts/ci/check-pins.sh).
ARG FLUTTER_VERSION=3.47.9
RUN cd /sdks/flutter && git fetch -q --depth 1 origin "refs/tags/${FLUTTER_VERSION}:refs/tags/${FLUTTER_VERSION}" && git checkout -q "${FLUTTER_VERSION}" && flutter --version
# Mirror the engine's fallback fonts (Roboto + Noto, URL list read from the pinned SDK) so browsers never call
# fonts.gstatic.com: the apps bundle no text font, so without this every glyph is fetched from Google at runtime.
# Build-time fetch only; curl -f fails the build if a pinned font URL disappears.
RUN grep -rhoE "[a-z0-9]+/v[0-9]+/[A-Za-z0-9_.-]+\.woff2" /sdks/flutter/engine/src/flutter/lib/web_ui/lib/src/engine | sort -u > /tmp/webfonts.txt \
 && test "$(wc -l < /tmp/webfonts.txt)" -ge 700 \
 && mkdir /webfonts && cd /webfonts \
 && xargs -P 8 -I{} curl -fsS --retry 3 --create-dirs -o {} "https://fonts.gstatic.com/s/{}" < /tmp/webfonts.txt
WORKDIR /src/app
COPY app/ .
# Same bootstrap for both apps: points the engine's fallback fonts at /webfonts/ (see deploy/flutter_bootstrap.js).
COPY deploy/flutter_bootstrap.js apps/console/web/flutter_bootstrap.js
COPY deploy/flutter_bootstrap.js apps/customer/web/flutter_bootstrap.js
# Both web builds self-host CanvasKit/skwasm (--no-web-resources-cdn) so the CSP needs no Google CDN,
# and compile with --wasm (JavaScript fallback included) so the CSP needs no 'unsafe-eval'.
# --enforce-lockfile: exactly the package versions in app/pubspec.lock, or the build fails.
RUN python3 -I tool/gen_arb.py \
 && flutter pub get --enforce-lockfile \
 && (cd packages/core && flutter gen-l10n) \
 && cd apps/console && flutter build web --release --wasm --no-web-resources-cdn --base-href /admin/ \
 && cd ../customer && flutter build web --release --wasm --no-web-resources-cdn --base-href /app/
# The shells take the business name and icons from the API at serve time (Caddy templates + httpInclude). Fail the
# image if the web build ever drops those includes from index.html; the fix would be to inject them here instead.
RUN for a in console customer; do \
      if grep -q 'httpInclude' "apps/$a/web/index.html"; then grep -q 'httpInclude' "apps/$a/build/web/index.html" \
        || { echo "the web build dropped the httpInclude lines from apps/$a/build/web/index.html" >&2; exit 1; }; fi; \
    done

FROM --platform=$BUILDPLATFORM caddy:2.11.9-builder@sha256:345264d8c6b50f44f4a4d507d4e6b6bcae40eef2f210d70f76e717fd8cc31baf AS caddy
# The stock 2.11.9 binary ships golang.org/x/net and klauspost/compress with fixed HIGH advisories
# (GO-2026-6860, GO-2026-6865). Same Caddy version, rebuilt against the patched modules.
# shortcut: pinned by hand, drop the replaces once a Caddy release ships these versions.
# Go cross-compiles on the build machine: under QEMU emulation the arm64 build stalled.
ARG TARGETOS
ARG TARGETARCH
RUN GOOS=$TARGETOS GOARCH=$TARGETARCH CGO_ENABLED=0 xcaddy build v2.11.9 --output /usr/bin/caddy \
      --replace golang.org/x/net=golang.org/x/net@v0.62.0 \
      --replace github.com/klauspost/compress=github.com/klauspost/compress@v1.20.4

# Runs as root inside the container, but deploy/docker-compose.yml drops every capability except
# NET_BIND_SERVICE and makes the filesystem read-only. A non-root USER here would lock existing servers
# out of their root-owned certificate volume on the next `dahae update`.
FROM caddy:2.11.9@sha256:4e2425f6009577e7c651010f16679c28f7d12df3e7b1914f941a7a3370746b3c
COPY --from=caddy /usr/bin/caddy /usr/bin/caddy
# Security fixes released since the base digest (the release scan blocks on fixable HIGH/CRITICAL).
RUN apk upgrade --no-cache
# shortcut: OpenSSL 3.5.9-r1 (CVE-2026-88823, -95691, -95692, -107496) is only in edge so far; drop this line
# once the base Alpine branch ships it.
RUN apk add --no-cache --repository=https://dl-cdn.alpinelinux.org/alpine/edge/main "libssl3>=3.5.9-r1" "libcrypto3>=3.5.9-r1"
ARG APP_VERSION=dev
LABEL org.opencontainers.image.source=https://github.com/EricWijesinghe/Dahae-install org.opencontainers.image.version=$APP_VERSION
COPY deploy/Caddyfile /etc/caddy/Caddyfile
COPY website/ /srv/website/
COPY --from=console /src/app/apps/console/build/web/ /srv/console/
COPY --from=console /src/app/apps/customer/build/web/ /srv/app/
COPY --from=console /webfonts/ /srv/webfonts/
