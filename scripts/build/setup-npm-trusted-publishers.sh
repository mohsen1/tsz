#!/bin/bash
# Configure npm trusted publishing for the npm packages produced by
# build-npm-packages.sh.
#
# npm currently requires the package record to exist before `npm trust` can
# attach a GitHub Actions publisher. If a package is brand new, publish it once
# with an npm token, then run this script and remove token-based publishing.

set -euo pipefail

REPO="${REPO:-tsz-org/tsz}"
WORKFLOW_FILE="${WORKFLOW_FILE:-npm-publish.yml}"
REGISTRY="${REGISTRY:-https://registry.npmjs.org}"

PACKAGES=(
  "@mohsen-azimi/try-tsz-darwin-arm64"
  "@mohsen-azimi/try-tsz-darwin-x64"
  "@mohsen-azimi/try-tsz-linux-x64"
  "@mohsen-azimi/try-tsz-linux-arm64"
  "@mohsen-azimi/try-tsz-win32-x64"
  "@mohsen-azimi/try-tsz-win32-arm64"
  "try-tsz"
)

for package in "${PACKAGES[@]}"; do
  echo "==> Configuring trusted publisher for $package"
  npx --yes npm@latest trust github "$package" \
    --repo "$REPO" \
    --file "$WORKFLOW_FILE" \
    --allow-publish \
    --yes \
    --registry "$REGISTRY"
done
