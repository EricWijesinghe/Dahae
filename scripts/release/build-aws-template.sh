#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Builds the AWS CloudFormation template for one release: copies deploy/aws/dahae.cfn.yaml with the pinned install.sh
# URL and its SHA-256 embedded. The server's first boot downloads that install.sh and refuses to run it unless the hash matches.
# Usage: build-aws-template.sh <install.sh> <raw install.sh URL> <out dir>
set -euo pipefail
install=${1:?usage: build-aws-template.sh <install.sh> <url> <out dir>}
url=${2:?usage: build-aws-template.sh <install.sh> <url> <out dir>}
out=${3:?usage: build-aws-template.sh <install.sh> <url> <out dir>}
root=$(cd "$(dirname "$0")/../.." && pwd)
src="$root/deploy/aws/dahae.cfn.yaml"

# shellcheck source=installer-pin.sh
. "$root/scripts/release/installer-pin.sh"
hash=$(installer_pin "$install" "$url") || exit 1
[ -f "$src" ] || { echo "::error::$src missing" >&2; exit 1; }

mkdir -p "$out"
sed -e "s|__INSTALL_SH_SHA256__|$hash|g" -e "s|__INSTALL_URL__|$url|g" "$src" > "$out/dahae-aws.yaml"
if grep -q '__INSTALL_' "$out/dahae-aws.yaml"; then
  echo "::error::placeholder left in $out/dahae-aws.yaml" >&2
  exit 1
fi
echo "aws template written: $out/dahae-aws.yaml (install.sh sha256 $hash)"
