#!/bin/bash
# Build WASM for nodejs and copy to local pkg directory
# Must be run from TypeScript root directory

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "🔨 Building WASM for Node.js..."

docker run --rm \
  -v "$PROJECT_ROOT:/source:ro" \
  -v "$PROJECT_ROOT/pkg:/output" \
  -v cargo-registry:/usr/local/cargo/registry \
  -v cargo-git:/usr/local/cargo/git \
  rust:latest sh -c "
    cargo install wasm-pack --locked 2>/dev/null
    mkdir -p /app
    cp -r /source/* /app/
    cd /app
    wasm-pack build --target nodejs
    cp -r /app/pkg/* /output/
  "

# Note: TypeScript lib files are now embedded in the WASM binary.
# They are fetched from npm by scripts/generate-lib-assets.mjs and
# compiled into the binary via src/embedded_libs.rs.
# No separate lib file copying is needed.

echo "✅ WASM built successfully to $PROJECT_ROOT/pkg/"
