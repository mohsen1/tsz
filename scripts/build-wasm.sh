#!/bin/bash
# Build WASM for nodejs and copy to local pkg directory

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "Building WASM for Node.js..."

# Check for wasm-pack
if ! command -v wasm-pack &>/dev/null; then
    echo "Error: wasm-pack is not installed."
    echo "Install with: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh"
    exit 1
fi

cd "$PROJECT_ROOT"

wasm-pack build --target nodejs --out-dir pkg

# Note: TypeScript lib files are now embedded in the WASM binary.
# They are fetched from npm by scripts/generate-lib-assets.mjs and
# compiled into the binary via src/embedded_libs.rs.
# No separate lib file copying is needed.

echo "WASM built successfully to $PROJECT_ROOT/pkg/"
