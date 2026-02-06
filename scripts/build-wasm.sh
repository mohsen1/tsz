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

# Note: Lib files are loaded at runtime from the TypeScript submodule's built/local/ directory.

echo "WASM built successfully to $PROJECT_ROOT/pkg/"
