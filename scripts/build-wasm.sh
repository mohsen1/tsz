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

LIB_SRC="$PROJECT_ROOT/TypeScript/lib"
if [ -d "$LIB_SRC" ]; then
  echo "📦 Copying TypeScript lib files (packaged)..."
  rm -rf "$PROJECT_ROOT/pkg/lib"
  mkdir -p "$PROJECT_ROOT/pkg/lib"
  cp -R "$LIB_SRC/." "$PROJECT_ROOT/pkg/lib/"
elif [ -d "$PROJECT_ROOT/TypeScript/src/lib" ]; then
  echo "📦 Copying TypeScript lib files (source)..."
  rm -rf "$PROJECT_ROOT/pkg/lib"
  mkdir -p "$PROJECT_ROOT/pkg/lib"
  cp -R "$PROJECT_ROOT/TypeScript/src/lib/." "$PROJECT_ROOT/pkg/lib/"
else
  echo "⚠️  TypeScript lib directory not found; skipping lib copy"
fi

echo "✅ WASM built successfully to $PROJECT_ROOT/pkg/"
