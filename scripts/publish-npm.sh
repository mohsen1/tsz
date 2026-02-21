#!/bin/bash
# Publish @mohsen-azimi/tsz-dev to npm.
#
# Usage:
#   ./scripts/publish-npm.sh            # publish (requires `npm login` first)
#   ./scripts/publish-npm.sh --dry-run  # verify tarball contents without publishing

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PKG="$PROJECT_ROOT/pkg"

DRY_RUN=0
for arg in "$@"; do
    case "$arg" in
        --dry-run) DRY_RUN=1 ;;
        *) echo "Unknown argument: $arg"; exit 1 ;;
    esac
done

# ---------------------------------------------------------------------------
# Step 1: Build both WASM targets
# ---------------------------------------------------------------------------
echo "==> Building WASM targets..."
bash "$SCRIPT_DIR/build-wasm.sh"

# ---------------------------------------------------------------------------
# Step 2: Sanity-check the assembled package
# ---------------------------------------------------------------------------
echo "==> Verifying package contents..."

required_files=(
    "$PKG/package.json"
    "$PKG/node/tsz_wasm.js"
    "$PKG/node/tsz_wasm.d.ts"
    "$PKG/node/tsz_wasm_bg.wasm"
    "$PKG/bundler/tsz_wasm.js"
    "$PKG/bundler/tsz_wasm.d.ts"
    "$PKG/bundler/tsz_wasm_bg.wasm"
    "$PKG/bin/tsz.js"
    "$PKG/bin/tsz-server.js"
    "$PKG/LICENSE.txt"
)

missing=0
for f in "${required_files[@]}"; do
    if [ ! -f "$f" ]; then
        echo "  MISSING: $f"
        missing=1
    fi
done

if [ "$missing" -eq 1 ]; then
    echo "Error: some expected files are missing — aborting."
    exit 1
fi

echo "  All required files present."

# ---------------------------------------------------------------------------
# Step 3: Publish (or dry-run)
# ---------------------------------------------------------------------------
if [ "$DRY_RUN" -eq 1 ]; then
    echo "==> Dry-run: listing files that would be published..."
    cd "$PKG"
    npm publish --dry-run --access public
else
    echo "==> Publishing @mohsen-azimi/tsz-dev to npm..."
    cd "$PKG"
    npm publish --access public
    echo "==> Published successfully!"
fi
