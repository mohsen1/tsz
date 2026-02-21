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
  "exports": {
    ".": {
      "require": "./node/tsz_wasm.js",
      "import": "./bundler/tsz_wasm.js",
      "types": "./node/tsz_wasm.d.ts"
    }
  },
  "files": ["node/", "bundler/", "LICENSE.txt"]
}
EOF

# Copy root LICENSE into pkg/ so it is included in the npm tarball
cp "$PROJECT_ROOT/LICENSE.txt" "$PKG/LICENSE.txt"

# Note: TypeScript stdlib lib files (lib.es5.d.ts, lib.dom.d.ts, etc.) are
# passed to TsProgram at runtime via addLibFile() — they are NOT bundled in
# this package.  See crates/tsz-wasm/src/wasm_api/program.rs for details.

echo "WASM built successfully.  Package assembled in $PKG/"
