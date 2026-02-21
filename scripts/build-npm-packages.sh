#!/bin/bash
# Build and assemble all npm packages for @mohsen-azimi/tsz distribution.
#
# Usage:
#   ./scripts/build-npm-packages.sh                  # build for current platform only (default)
#   ./scripts/build-npm-packages.sh --local           # same as above
#   ./scripts/build-npm-packages.sh --all             # build for all 6 platforms
#   ./scripts/build-npm-packages.sh --wasm-only       # only build WASM, skip native binaries
#   ./scripts/build-npm-packages.sh --native-only     # only build native, skip WASM
#   ./scripts/build-npm-packages.sh --dry-run         # show what would be built
#   ./scripts/build-npm-packages.sh --skip-build      # assemble only (binaries already built)
#
# CI workflow:
#   Each platform runner builds its own native binary, then a final job
#   runs --skip-build to assemble all pre-built artifacts into npm packages.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
NPM_DIR="$PROJECT_ROOT/npm"
MAIN_PKG="$NPM_DIR/tsz"
CARGO_PROFILE="${CARGO_PROFILE:-dist-fast}"

# ─── Parse arguments ──────────────────────────────────────────────────────────
BUILD_MODE="local"  # local | all
DRY_RUN=0
SKIP_BUILD=0
WASM_ONLY=0
NATIVE_ONLY=0

for arg in "$@"; do
  case "$arg" in
    --local)       BUILD_MODE="local" ;;
    --all)         BUILD_MODE="all" ;;
    --dry-run)     DRY_RUN=1 ;;
    --skip-build)  SKIP_BUILD=1 ;;
    --wasm-only)   WASM_ONLY=1 ;;
    --native-only) NATIVE_ONLY=1 ;;
    *) echo "Unknown argument: $arg"; exit 1 ;;
  esac
done

# ─── Platform definitions ─────────────────────────────────────────────────────
# Format: "npm_suffix rust_target"
PLATFORMS=(
  "darwin-arm64  aarch64-apple-darwin"
  "darwin-x64    x86_64-apple-darwin"
  "linux-x64     x86_64-unknown-linux-gnu"
  "linux-arm64   aarch64-unknown-linux-gnu"
  "win32-x64     x86_64-pc-windows-msvc"
  "win32-arm64   aarch64-pc-windows-msvc"
)

# Binaries to ship (from tsz-cli crate)
BINARIES=(tsz tsz-server)

# ─── Detect current platform ──────────────────────────────────────────────────
detect_current_platform() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "$os" in
    Darwin) os="darwin" ;;
    Linux)  os="linux" ;;
    MINGW*|MSYS*|CYGWIN*) os="win32" ;;
    *) echo "Unknown OS: $os"; return 1 ;;
  esac

  case "$arch" in
    x86_64|amd64) arch="x64" ;;
    arm64|aarch64) arch="arm64" ;;
    *) echo "Unknown arch: $arch"; return 1 ;;
  esac

  echo "${os}-${arch}"
}

# Map npm platform suffix to Rust target triple
get_rust_target() {
  local suffix="$1"
  for entry in "${PLATFORMS[@]}"; do
    local npm_suffix rust_target
    read -r npm_suffix rust_target <<< "$entry"
    if [ "$npm_suffix" = "$suffix" ]; then
      echo "$rust_target"
      return 0
    fi
  done
  return 1
}

# ─── Extract version ──────────────────────────────────────────────────────────
CARGO_VERSION=$(grep '^version' "$PROJECT_ROOT/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
echo "==> Version: $CARGO_VERSION"

# ─── Determine which platforms to build ───────────────────────────────────────
BUILD_PLATFORMS=()
if [ "$BUILD_MODE" = "local" ]; then
  CURRENT=$(detect_current_platform)
  echo "==> Building for current platform: $CURRENT"
  BUILD_PLATFORMS=("$CURRENT")
else
  echo "==> Building for all platforms"
  for entry in "${PLATFORMS[@]}"; do
    npm_suffix=""
    read -r npm_suffix _ <<< "$entry"
    BUILD_PLATFORMS+=("$npm_suffix")
  done
fi

if [ "$DRY_RUN" -eq 1 ]; then
  echo ""
  echo "Dry run — would build:"
  if [ "$NATIVE_ONLY" -ne 1 ]; then
    echo "  WASM: node + bundler targets"
  fi
  if [ "$WASM_ONLY" -ne 1 ]; then
    for p in "${BUILD_PLATFORMS[@]}"; do
      rt=$(get_rust_target "$p")
      echo "  Native: $p ($rt)"
    done
  fi
  echo ""
  echo "Packages:"
  echo "  @mohsen-azimi/tsz (main package)"
  for p in "${BUILD_PLATFORMS[@]}"; do
    echo "  @mohsen-azimi/tsz-$p"
  done
  exit 0
fi

# ─── Step 1: Build WASM ──────────────────────────────────────────────────────
if [ "$NATIVE_ONLY" -ne 1 ] && [ "$SKIP_BUILD" -ne 1 ]; then
  echo ""
  echo "==> Building WASM targets..."

  if ! command -v wasm-pack &>/dev/null; then
    echo "Error: wasm-pack is not installed."
    echo "Install with: curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh"
    exit 1
  fi

  cd "$PROJECT_ROOT"
  cp "$PROJECT_ROOT/LICENSE.txt" "$PROJECT_ROOT/crates/tsz-wasm/LICENSE.txt"

  # Clean previous WASM build outputs to avoid stale files
  rm -rf "$MAIN_PKG/wasm/node" "$MAIN_PKG/wasm/bundler"

  # Build Node.js target (CJS)
  echo "  Building Node.js (CJS) target..."
  wasm-pack build crates/tsz-wasm --target nodejs --out-dir "$MAIN_PKG/wasm/node"

  # Build bundler target (ESM)
  echo "  Building bundler (ESM) target..."
  wasm-pack build crates/tsz-wasm --target bundler --out-dir "$MAIN_PKG/wasm/bundler"

  # wasm-pack leaves .gitignore files that break npm publish
  rm -f "$MAIN_PKG/wasm/node/.gitignore" "$MAIN_PKG/wasm/bundler/.gitignore"
  # wasm-pack also writes package.json files we don't need
  rm -f "$MAIN_PKG/wasm/node/package.json" "$MAIN_PKG/wasm/bundler/package.json"

  echo "  WASM build complete."
fi

# ─── Step 2: Build native binaries ───────────────────────────────────────────
if [ "$WASM_ONLY" -ne 1 ] && [ "$SKIP_BUILD" -ne 1 ]; then
  echo ""
  echo "==> Building native binaries..."

  for platform_suffix in "${BUILD_PLATFORMS[@]}"; do
    rust_target=$(get_rust_target "$platform_suffix")
    echo "  Building for $platform_suffix ($rust_target)..."

    cargo build --profile "$CARGO_PROFILE" -p tsz-cli --target "$rust_target"

    # Copy binaries to the platform package
    pkg_bin="$NPM_DIR/@mohsen-azimi/tsz-$platform_suffix/bin"
    mkdir -p "$pkg_bin"

    for bin_name in "${BINARIES[@]}"; do
      ext=""
      if [[ "$platform_suffix" == win32-* ]]; then
        ext=".exe"
      fi

      # Cargo uses the profile name as-is for the output directory
      src="$PROJECT_ROOT/target/$rust_target/dist-fast/$bin_name$ext"
      if [ ! -f "$src" ]; then
        src="$PROJECT_ROOT/target/$rust_target/release/$bin_name$ext"
      fi

      if [ -f "$src" ]; then
        cp "$src" "$pkg_bin/$bin_name$ext"
        chmod +x "$pkg_bin/$bin_name$ext"
        echo "    Copied $bin_name$ext ($(du -h "$pkg_bin/$bin_name$ext" | cut -f1))"
      else
        echo "    WARNING: binary not found: $bin_name$ext"
        echo "    Searched: $PROJECT_ROOT/target/$rust_target/{dist-fast,release}/$bin_name$ext"
      fi
    done
  done
fi

# ─── Step 3: Assemble main package ───────────────────────────────────────────
echo ""
echo "==> Assembling main package..."

# Update version in main package.json (pass values via env to avoid injection)
cd "$PROJECT_ROOT"
TSZ_VERSION="$CARGO_VERSION" TSZ_PKG_FILE="$MAIN_PKG/package.json" node -e '
  const fs = require("fs");
  const version = process.env.TSZ_VERSION;
  const pkgFile = process.env.TSZ_PKG_FILE;
  const pkg = JSON.parse(fs.readFileSync(pkgFile, "utf8"));
  pkg.version = version;
  for (const dep of Object.keys(pkg.optionalDependencies || {})) {
    pkg.optionalDependencies[dep] = version;
  }
  fs.writeFileSync(pkgFile, JSON.stringify(pkg, null, 2) + "\n");
'

# Update version in each platform package.json
for entry in "${PLATFORMS[@]}"; do
  read -r npm_suffix _ <<< "$entry"
  pkg_json="$NPM_DIR/@mohsen-azimi/tsz-$npm_suffix/package.json"
  if [ -f "$pkg_json" ]; then
    TSZ_VERSION="$CARGO_VERSION" TSZ_PKG_FILE="$pkg_json" node -e '
      const fs = require("fs");
      const version = process.env.TSZ_VERSION;
      const pkgFile = process.env.TSZ_PKG_FILE;
      const pkg = JSON.parse(fs.readFileSync(pkgFile, "utf8"));
      pkg.version = version;
      fs.writeFileSync(pkgFile, JSON.stringify(pkg, null, 2) + "\n");
    '
  fi
done

# Copy LICENSE
cp "$PROJECT_ROOT/LICENSE.txt" "$MAIN_PKG/LICENSE.txt"

# Bundle TypeScript lib files
LIB_ASSETS="$PROJECT_ROOT/src/lib-assets"
if [ -d "$LIB_ASSETS" ]; then
  echo "  Bundling TypeScript lib files..."
  mkdir -p "$MAIN_PKG/lib-assets"
  cp "$LIB_ASSETS"/*.d.ts "$MAIN_PKG/lib-assets/"
  cp "$LIB_ASSETS/lib_manifest.json" "$MAIN_PKG/lib-assets/"
  echo "  Copied $(ls "$MAIN_PKG/lib-assets"/*.d.ts 2>/dev/null | wc -l | tr -d ' ') lib files"
else
  echo "  WARNING: lib-assets directory not found at $LIB_ASSETS"
fi

# Make launcher scripts executable
chmod +x "$MAIN_PKG/bin/tsz.js" "$MAIN_PKG/bin/tsz-server.js"

echo ""
echo "==> Build complete!"
echo "    Main package: $MAIN_PKG"
for platform_suffix in "${BUILD_PLATFORMS[@]}"; do
  echo "    Platform:     $NPM_DIR/@mohsen-azimi/tsz-$platform_suffix"
done
echo ""
echo "To test locally:"
echo "  cd $MAIN_PKG && npm link"
echo "  tsz --noEmit"
echo ""
echo "To publish:"
echo "  # Publish platform packages first:"
for platform_suffix in "${BUILD_PLATFORMS[@]}"; do
  echo "  cd $NPM_DIR/@mohsen-azimi/tsz-$platform_suffix && npm publish --access public"
done
echo "  # Then publish main package:"
echo "  cd $MAIN_PKG && npm publish --access public"
