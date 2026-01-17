#!/bin/bash
# Build WASM for nodejs and copy to local pkg directory
# Must be run from TypeScript root directory

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "🔨 Building WASM for Node.js..."

docker run --rm \
  -v "$SCRIPT_DIR:/source:ro" \
  -v "$SCRIPT_DIR/pkg:/output" \
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

echo "✅ WASM built successfully to $SCRIPT_DIR/pkg/"
