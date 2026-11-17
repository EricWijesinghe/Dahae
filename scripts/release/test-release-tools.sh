#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Tests for the release signing scripts. No network and no real cosign: a stub records its arguments.
# Good input must pass and tampered input must fail. Usage: bash scripts/release/test-release-tools.sh
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
R="$ROOT/scripts/release"
SB=$(mktemp -d)
trap 'rm -rf "$SB"' EXIT
PASS=0 FAIL=0

t()  { if "${@:2}" >/dev/null 2>&1; then PASS=$((PASS + 1)); else FAIL=$((FAIL + 1)); echo "FAIL: $1"; fi; }
nt() { if "${@:2}" >/dev/null 2>&1; then FAIL=$((FAIL + 1)); echo "FAIL (should fail): $1"; else PASS=$((PASS + 1)); fi; }

openssl genpkey -algorithm ed25519 -out "$SB/priv.pem" 2>/dev/null
openssl pkey -in "$SB/priv.pem" -pubout -out "$SB/pub.pem" 2>/dev/null
openssl genpkey -algorithm ed25519 -out "$SB/other.pem" 2>/dev/null

mkrel() { # mkrel <dir> : a fake published release (install.sh, agent.sh, SHA256SUMS)
  mkdir -p "$1"
  printf 'echo installer\n' > "$1/install.sh"
  printf 'echo agent\n' > "$1/agent.sh"
  (cd "$1" && sha256sum install.sh agent.sh > SHA256SUMS)
}
mkrel "$SB/rel"

# ---- manifest: sign, then verify ----
t  "manifest: sign good release"   bash "$R/make-manifest.sh" v1.2.3 "$SB/rel" "$SB/priv.pem"
t  "manifest: verify good release" bash "$R/verify-manifest.sh" "$SB/rel" "$SB/pub.pem" v1.2.3
nt "manifest: wrong expected tag"  bash "$R/verify-manifest.sh" "$SB/rel" "$SB/pub.pem" v9.9.9
nt "manifest: bad tag refused at signing" bash "$R/make-manifest.sh" main "$SB/rel" "$SB/priv.pem"
nt "manifest: missing key refused" bash "$R/make-manifest.sh" v1.2.3 "$SB/rel" "$SB/none.pem"

# tampering: each case copies the signed release first
tamper() { rm -rf "$SB/tmp"; cp -r "$SB/rel" "$SB/tmp"; "$@"; }
nt "tampered install.sh" tamper bash -c "echo evil >> '$SB/tmp/install.sh'; bash '$R/verify-manifest.sh' '$SB/tmp' '$SB/pub.pem'"
nt "tampered agent.sh" tamper bash -c "echo evil >> '$SB/tmp/agent.sh'; bash '$R/verify-manifest.sh' '$SB/tmp' '$SB/pub.pem'"
nt "tampered manifest tag" tamper bash -c "sed -i 's/v1.2.3/v1.2.4/' '$SB/tmp/installer_manifest.json'; bash '$R/verify-manifest.sh' '$SB/tmp' '$SB/pub.pem'"
nt "tampered SHA256SUMS line" tamper bash -c "sed -i 's/^[0-9a-f]\{64\}/$(printf '0%.0s' {1..64})/' '$SB/tmp/SHA256SUMS'; bash '$R/verify-manifest.sh' '$SB/tmp' '$SB/pub.pem'"
nt "manifest signed by another key" tamper bash -c "bash '$R/make-manifest.sh' v1.2.3 '$SB/tmp' '$SB/other.pem'; bash '$R/verify-manifest.sh' '$SB/tmp' '$SB/pub.pem'"
nt "manifest without signature" tamper bash -c "rm '$SB/tmp/installer_manifest.json.sig'; bash '$R/verify-manifest.sh' '$SB/tmp' '$SB/pub.pem'"

# ---- cosign bundle: a stub records its arguments and exits with STUB_RC ----
mkdir -p "$SB/bin"
cat > "$SB/bin/cosign" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "$COSIGN_LOG"
exit "${STUB_RC:-0}"
STUB
chmod +x "$SB/bin/cosign"
export COSIGN_LOG="$SB/cosign.log"
printf 'sums\n' > "$SB/SHA256SUMS"
: > "$SB/SHA256SUMS.bundle"

t  "sign-blob: calls cosign sign-blob with bundle" env PATH="$SB/bin:$PATH" bash "$R/sign-blob.sh" "$SB/SHA256SUMS"
t  "sign-blob: bundle path in the call" grep -q -- "--bundle $SB/SHA256SUMS.bundle" "$COSIGN_LOG"
nt "sign-blob: cosign failure is fatal" env PATH="$SB/bin:$PATH" STUB_RC=1 bash "$R/sign-blob.sh" "$SB/SHA256SUMS"
t  "verify-blob: good bundle accepted" env PATH="$SB/bin:$PATH" bash "$R/verify-blob.sh" "$SB/SHA256SUMS" owner/app
t  "verify-blob: identity pins the release workflow of the repo" grep -qF 'release\.yml@refs/tags/v[0-9]' "$COSIGN_LOG"
escaped_identity() { env PATH="$SB/bin:$PATH" bash "$R/verify-blob.sh" "$SB/SHA256SUMS" my-org/app.v2 && grep -qF 'github\.com/my-org/app\.v2/' "$COSIGN_LOG"; }
t  "verify-blob: dots in the repo name are escaped" escaped_identity
nt "verify-blob: rejected signature" env PATH="$SB/bin:$PATH" STUB_RC=1 bash "$R/verify-blob.sh" "$SB/SHA256SUMS" owner/app
nt "verify-blob: bad repo name refused" env PATH="$SB/bin:$PATH" bash "$R/verify-blob.sh" "$SB/SHA256SUMS" 'owner/app;rm'

# ---- install page: built with the hash and URL of the release ----
mkdir -p "$SB/page"
url="https://raw.githubusercontent.com/owner/dist/v1.2.3/install.sh"
t  "page: builds for a pinned tag URL" bash "$R/build-install-page.sh" "$SB/rel/install.sh" "$url" "$SB/page/out"
t  "page: embeds the install.sh hash" grep -q "$(sha256sum "$SB/rel/install.sh" | cut -d' ' -f1)" "$SB/page/out/index.html"
t  "page: embeds the install URL" grep -qF "$url" "$SB/page/out/index.html"
nt "page: branch URL refused" bash "$R/build-install-page.sh" "$SB/rel/install.sh" "https://raw.githubusercontent.com/owner/dist/main/install.sh" "$SB/page/out2"
nt "page: non-raw host refused" bash "$R/build-install-page.sh" "$SB/rel/install.sh" "https://evil.example/v1.2.3/install.sh" "$SB/page/out3"
nt "page: missing installer refused" bash "$R/build-install-page.sh" "$SB/none.sh" "$url" "$SB/page/out4"

# ---- AWS template: same pin as the install page, every placeholder filled, the first boot runs the pinned installer ----
aws_tpl="$ROOT/deploy/aws/dahae.cfn.yaml"
t  "aws: template source has both placeholders" grep -q '__INSTALL_URL__' "$aws_tpl" && grep -q '__INSTALL_SH_SHA256__' "$aws_tpl"
t  "aws: template builds for a pinned tag URL" bash "$R/build-aws-template.sh" "$SB/rel/install.sh" "$url" "$SB/aws/out"
t  "aws: template embeds the install.sh hash" grep -q "$(sha256sum "$SB/rel/install.sh" | cut -d' ' -f1)" "$SB/aws/out/dahae-aws.yaml"
t  "aws: template embeds the install URL" grep -qF "$url" "$SB/aws/out/dahae-aws.yaml"
t  "aws: template checks the hash before running" grep -qF 'sha256sum -c -' "$SB/aws/out/dahae-aws.yaml"
nt "aws: branch URL refused" bash "$R/build-aws-template.sh" "$SB/rel/install.sh" "https://raw.githubusercontent.com/owner/dist/main/install.sh" "$SB/aws/out2"
nt "aws: missing installer refused" bash "$R/build-aws-template.sh" "$SB/none.sh" "$url" "$SB/aws/out3"
# the deployer may create roles only inside dahae-boundary, so every role in the template must carry it
every_role_bounded() {
  local roles; roles=$(grep -c 'Type: AWS::IAM::Role$' "$1")
  [ "$roles" -gt 0 ] && [ "$roles" = "$(grep -c 'PermissionsBoundary: .*policy/dahae-boundary' "$1")" ]
}
t  "aws: every role carries the dahae-boundary permissions boundary" every_role_bounded "$SB/aws/out/dahae-aws.yaml"
# shellcheck disable=SC2016 # ${GitHubRepoIds} is CloudFormation text, not a shell expansion
t  "aws: deploy role trusts the immutable OIDC subject" grep -qF 'sub: !Sub "repo:${GitHubRepoIds}:environment:' "$SB/aws/out/dahae-aws.yaml"
t  "aws: deployer creates roles only with the boundary" grep -qF '"iam:PermissionsBoundary": "arn:aws:iam::*:policy/dahae-boundary"' "$ROOT/deploy/aws/deployer-iam-policy.json"
# An import change set would tag another instance aws:cloudformation:stack-name=dahae-* and open StartSession on it.
t  "aws: deployer change sets cannot import resources" grep -qF '"cloudformation:ImportResourceTypes": "true"' "$ROOT/deploy/aws/deployer-iam-policy.json"
fits_iam_limit() { [ "$(tr -d '[:space:]' < "$1" | wc -c)" -le 6144 ]; } # managed policy limit, whitespace not counted
t  "aws: deployer policy fits the IAM size limit" fits_iam_limit "$ROOT/deploy/aws/deployer-iam-policy.json"
t  "aws: boundary policy fits the IAM size limit" fits_iam_limit "$ROOT/deploy/aws/dahae-boundary-policy.json"

# ---- install page -> installer: cloud-init sets only variables install.sh reads, no answers file ----
page="$ROOT/deploy/install-page/index.html"
inst="$ROOT/deploy/install.sh"
page_vars=$(grep -o 'MALKKEUM_[A-Z_]*=' "$page" | sort -u)
t  "page: cloud-init sets installer variables" test -n "$page_vars"
for v in $page_vars; do
  n=${v%=}
  t "page: installer reads $n" grep -qF "{$n:-" "$inst"
done
nt "page: no answers file (the setup code comes from the page itself)" grep -qE 'dahae-answers|MALKKEUM_ANSWERS' "$page"

# ---- delivery re-run: the same release again changes no commit, and the tag stays where it was ----
pd="$SB/pd"; mkdir -p "$pd/bin"
git init -q --bare "$pd/dist.git"
cat > "$pd/bin/cosign" <<'STUB'
#!/usr/bin/env bash
# Stub for `cosign sign-blob --bundle <file> <blob>`: each call writes a different bundle, as keyless signing does.
while [ $# -gt 0 ]; do
  if [ "$1" = --bundle ]; then printf 'bundle %s %s\n' "$RANDOM" "$RANDOM" > "$2"; exit 0; fi
  shift
done
exit 1
STUB
chmod +x "$pd/bin/cosign"
publish_release() {
  env PATH="$pd/bin:$PATH" DIST_URL="$pd/dist.git" DIST_REPO=owner/dist VER=v9.9.9 \
    IMAGE_REPO=ghcr.io/owner SIGNER_REPO=owner/app SIGN_RELEASE=1 \
    MANIFEST_KEY_FILE="$SB/priv.pem" ASSET_DIR="$pd/assets" bash "$ROOT/deploy/publish-delivery.sh"
}
published() { git -C "$pd/dist.git" rev-parse --verify -q "$1"; }
t  "delivery: first publish of a release" publish_release
t  "delivery: tag published" published 'v9.9.9^{commit}'
t  "delivery: first run signs SHA256SUMS" git -C "$pd/dist.git" cat-file -e 'main:SHA256SUMS.bundle'
beta1=$(published refs/heads/beta); main1=$(published refs/heads/main); tag1=$(published 'v9.9.9^{commit}')
bundle1=$(cat "$pd/assets/SHA256SUMS.bundle")
t  "delivery: re-run of the same release succeeds" publish_release
t  "delivery: re-run keeps the beta commit" [ "$(published refs/heads/beta)" = "$beta1" ]
t  "delivery: re-run keeps the main commit" [ "$(published refs/heads/main)" = "$main1" ]
t  "delivery: re-run keeps the tag on its commit" [ "$(published 'v9.9.9^{commit}')" = "$tag1" ]
t  "delivery: re-run ships the same signature" [ "$(cat "$pd/assets/SHA256SUMS.bundle")" = "$bundle1" ]

# ---- syntax ----
for s in make-manifest.sh verify-manifest.sh sign-blob.sh verify-blob.sh build-install-page.sh build-aws-template.sh installer-pin.sh test-release-tools.sh; do
  t "syntax: $s" bash -n "$R/$s"
done
t "syntax: publish-delivery.sh" bash -n "$ROOT/deploy/publish-delivery.sh"

echo "release tools: $PASS passed, $FAIL failed"
[ "$FAIL" = 0 ]
