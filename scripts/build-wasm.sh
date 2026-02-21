#!/bin/bash
# Build WASM for Node.js (CJS) and bundler (ESM) targets, then assemble the
# unified @mohsen-azimi/tsz-dev package in pkg/.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PKG="$PROJECT_ROOT/pkg"

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------
if ! command -v wasm-pack &>/dev/null; then
    echo "Error: wasm-pack is not installed."
    echo "Install with: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh"
    exit 1
fi

cd "$PROJECT_ROOT"

# wasm-pack expects a LICENSE file in the crate directory it builds
cp "$PROJECT_ROOT/LICENSE.txt" "$PROJECT_ROOT/crates/tsz-wasm/LICENSE.txt"

# ---------------------------------------------------------------------------
# Build Node.js target  (CommonJS, synchronous WASM init)
# ---------------------------------------------------------------------------
echo "Building WASM for Node.js (CJS)..."
wasm-pack build crates/tsz-wasm --target nodejs  --out-dir "$PKG/node"

# ---------------------------------------------------------------------------
# Build bundler target  (ESM, for webpack / Vite / Rollup)
# ---------------------------------------------------------------------------
echo "Building WASM for bundler (ESM)..."
wasm-pack build crates/tsz-wasm --target bundler --out-dir "$PKG/bundler"

# ---------------------------------------------------------------------------
# Write unified package.json  (overwrites whatever wasm-pack left at pkg/)
# ---------------------------------------------------------------------------

# wasm-pack writes a `.gitignore` with `*` into each output dir, which causes
# `npm publish` to exclude all files inside those directories.  Remove them.
rm -f "$PKG/node/.gitignore" "$PKG/bundler/.gitignore"

# Extract version from workspace Cargo.toml so npm package stays in sync.
CARGO_VERSION=$(grep '^version' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')

echo "Writing unified package.json (version $CARGO_VERSION)..."
cat > "$PKG/package.json" <<EOF
{
  "name": "@mohsen-azimi/tsz-dev",
  "version": "$CARGO_VERSION",
  "description": "WebAssembly bindings for the tsz TypeScript compiler",
  "license": "Apache-2.0",
  "author": "Mohsen Azimi <mohsen@users.noreply.github.com>",
  "repository": {
    "type": "git",
    "url": "git+https://github.com/mohsenazimi/tsz.git"
  },
  "keywords": ["typescript", "type-checker", "compiler", "wasm"],
  "main": "node/tsz_wasm.js",
  "types": "node/tsz_wasm.d.ts",
  "bin": {
    "tsz": "./bin/tsz.js",
    "tsz-server": "./bin/tsz-server.js"
  },
  "exports": {
    ".": {
      "require": "./node/tsz_wasm.js",
      "import": "./bundler/tsz_wasm.js",
      "types": "./node/tsz_wasm.d.ts"
    }
  },
  "files": ["node/", "bundler/", "bin/", "lib-assets/", "LICENSE.txt"]
}
EOF

# ─── Copy and chmod bin scripts ───────────────────────────────────────────────
mkdir -p "$PKG/bin"
cp "$PROJECT_ROOT/wasm/bin/tsz.js"        "$PKG/bin/tsz.js"
cp "$PROJECT_ROOT/wasm/bin/tsz-server.js" "$PKG/bin/tsz-server.js"
chmod +x "$PKG/bin/tsz.js" "$PKG/bin/tsz-server.js"

# Copy root LICENSE into pkg/ so it is included in the npm tarball
cp "$PROJECT_ROOT/LICENSE.txt" "$PKG/LICENSE.txt"

# ─── Bundle TypeScript lib files ──────────────────────────────────────────────
# lib .d.ts files provide global type definitions (Array, String, Promise, etc.)
# The CLI wrapper loads them at startup via TsProgram.addLibFile().
LIB_ASSETS="$PROJECT_ROOT/src/lib-assets"
if [ -d "$LIB_ASSETS" ]; then
    echo "Bundling TypeScript lib files..."
    mkdir -p "$PKG/lib-assets"
    cp "$LIB_ASSETS"/*.d.ts "$PKG/lib-assets/"
    cp "$LIB_ASSETS/lib_manifest.json" "$PKG/lib-assets/"
    echo "  Copied $(ls "$PKG/lib-assets"/*.d.ts 2>/dev/null | wc -l | tr -d ' ') lib files"
else
    echo "WARNING: lib-assets directory not found at $LIB_ASSETS"
    echo "  The CLI will not have TypeScript built-in types (Array, String, etc.)"
fi

echo "WASM built successfully.  Package assembled in $PKG/"
