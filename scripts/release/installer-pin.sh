#!/usr/bin/env bash
# Copyright (c) 2026 Eric Wijesinghe. All rights reserved.
# SPDX-License-Identifier: LicenseRef-Proprietary

# Shared by build-install-page.sh and build-aws-template.sh (sourced, not run). installer_pin <install.sh> <url>
# checks that the URL is a pinned tag URL on raw.githubusercontent.com and prints the install.sh SHA-256.
# Exits non-zero (message on stderr) when the URL or file is wrong.
installer_pin() {
  local install=$1 url=$2 hash
  [[ $url =~ ^https://raw\.githubusercontent\.com/[A-Za-z0-9._-]+/[A-Za-z0-9._-]+/v[0-9][A-Za-z0-9._+-]*/install\.sh$ ]] \
    || { echo "::error::install URL must be a pinned tag URL on raw.githubusercontent.com: $url" >&2; return 1; }
  [ -f "$install" ] || { echo "::error::$install missing" >&2; return 1; }
  hash=$(sha256sum "$install" | cut -d' ' -f1)
  [[ $hash =~ ^[0-9a-f]{64}$ ]] || { echo "::error::could not hash $install" >&2; return 1; }
  printf '%s' "$hash"
}
