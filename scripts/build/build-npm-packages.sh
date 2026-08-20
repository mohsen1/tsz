#!/bin/bash
# Assemble private native npm packages for R0 process-contract validation.
# Publication and WASM packaging stay disabled during the clean-slate rewrite.
#
# Usage:
#   ./scripts/build/build-npm-packages.sh                  # build for current platform only (default)
#   ./scripts/build/build-npm-packages.sh --local           # same as above
#   ./scripts/build/build-npm-packages.sh --all             # build for all 6 platforms
#   ./scripts/build/build-npm-packages.sh --native-only     # compatibility alias; native is the only mode
#   ./scripts/build/build-npm-packages.sh --dry-run         # show what would be built
#   ./scripts/build/build-npm-packages.sh --skip-build      # assemble only (binaries already built)
#
# CI workflow:
#   Each platform runner builds its own native binary, then a final job
#   runs --skip-build to assemble all pre-built artifacts into npm packages.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
NPM_DIR="$PROJECT_ROOT/npm"
MAIN_PKG="$NPM_DIR/tsz"
TRY_PKG="$NPM_DIR/try-tsz"
CARGO_PROFILE="${CARGO_PROFILE:-dist-fast}"
CARGO_TARGET_ROOT="${CARGO_TARGET_DIR:-$PROJECT_ROOT/.target}"
if [[ "$CARGO_TARGET_ROOT" != /* ]]; then
  CARGO_TARGET_ROOT="$PROJECT_ROOT/$CARGO_TARGET_ROOT"
fi

# ─── Parse arguments ──────────────────────────────────────────────────────────
BUILD_MODE="local"  # local | all
DRY_RUN=0
SKIP_BUILD=0

for arg in "$@"; do
  case "$arg" in
    --local)       BUILD_MODE="local" ;;
    --all)         BUILD_MODE="all" ;;
    --dry-run)     DRY_RUN=1 ;;
    --skip-build)  SKIP_BUILD=1 ;;
    --wasm-only)
      echo "Error: WASM packaging is unavailable during the rewrite; WASM returns at R4." >&2
      exit 2
      ;;
    --native-only) ;;
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
BINARIES=(tsz tsz-server try-tsz)

extract_workspace_package_field() {
  local field="$1"
  awk -F '"' -v key="$field" '
    BEGIN { in_workspace_pkg = 0 }
    /^\[workspace\.package\]/ { in_workspace_pkg = 1; next }
    /^\[/ { if (in_workspace_pkg) exit }
    in_workspace_pkg && $1 ~ "^[[:space:]]*" key "[[:space:]]*=" {
      print $2
      exit
    }
  ' "$PROJECT_ROOT/Cargo.toml"
}

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
CARGO_VERSION="$(extract_workspace_package_field "version")"
if [ -z "$CARGO_VERSION" ]; then
  echo "Error: failed to read workspace.package.version from Cargo.toml"
  exit 1
fi
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
  for p in "${BUILD_PLATFORMS[@]}"; do
    rt=$(get_rust_target "$p")
    echo "  Native: $p ($rt)"
  done
  echo ""
  echo "Packages:"
  echo "  @mohsen-azimi/tsz (private R0 package)"
  echo "  try-tsz (private R0 package)"
  for p in "${BUILD_PLATFORMS[@]}"; do
    echo "  @mohsen-azimi/tsz-$p"
    echo "  @mohsen-azimi/try-tsz-$p"
  done
  exit 0
fi

# ─── Step 1: Build native binaries ───────────────────────────────────────────
if [ "$SKIP_BUILD" -ne 1 ]; then
  echo ""
  echo "==> Building native binaries..."

  for platform_suffix in "${BUILD_PLATFORMS[@]}"; do
    rust_target=$(get_rust_target "$platform_suffix")
    echo "  Building for $platform_suffix ($rust_target)..."

    CARGO_TARGET_DIR="$CARGO_TARGET_ROOT" \
      cargo build --profile "$CARGO_PROFILE" -p tsz-cli --target "$rust_target"

    # Copy binaries to the platform package
    pkg_bin="$NPM_DIR/@mohsen-azimi/tsz-$platform_suffix/bin"
    mkdir -p "$pkg_bin"
    try_pkg_bin="$NPM_DIR/@mohsen-azimi/try-tsz-$platform_suffix/bin"
    mkdir -p "$try_pkg_bin"

    for bin_name in "${BINARIES[@]}"; do
      ext=""
      if [[ "$platform_suffix" == win32-* ]]; then
        ext=".exe"
      fi

      # Cargo uses the profile name as-is for the output directory
      src="$CARGO_TARGET_ROOT/$rust_target/dist-fast/$bin_name$ext"
      if [ ! -f "$src" ]; then
        src="$CARGO_TARGET_ROOT/$rust_target/release/$bin_name$ext"
      fi

      if [ -f "$src" ]; then
        cp "$src" "$pkg_bin/$bin_name$ext"
        chmod +x "$pkg_bin/$bin_name$ext"
        if [ "$bin_name" = "try-tsz" ]; then
          cp "$src" "$try_pkg_bin/$bin_name$ext"
          chmod +x "$try_pkg_bin/$bin_name$ext"
        fi
        echo "    Copied $bin_name$ext ($(du -h "$pkg_bin/$bin_name$ext" | cut -f1))"
      else
        echo "    ERROR: binary not found: $bin_name$ext" >&2
        echo "    Searched: $CARGO_TARGET_ROOT/$rust_target/{dist-fast,release}/$bin_name$ext" >&2
        exit 1
      fi
    done
  done
fi

# ─── Step 2: Assemble main package ───────────────────────────────────────────
echo ""
echo "==> Assembling main package..."

# Generate main and platform package metadata (pass values via env to avoid injection)
cd "$PROJECT_ROOT"
mkdir -p "$MAIN_PKG/bin"
TSZ_VERSION="$CARGO_VERSION" NPM_DIR="$NPM_DIR" MAIN_PKG="$MAIN_PKG" node - <<'NODE'
const fs = require("fs");
const path = require("path");

const version = process.env.TSZ_VERSION;
const npmDir = process.env.NPM_DIR;
const mainPkg = process.env.MAIN_PKG;
const platforms = [
  { suffix: "darwin-arm64", os: "darwin", cpu: "arm64" },
  { suffix: "darwin-x64", os: "darwin", cpu: "x64" },
  { suffix: "linux-x64", os: "linux", cpu: "x64" },
  { suffix: "linux-arm64", os: "linux", cpu: "arm64" },
  { suffix: "win32-x64", os: "win32", cpu: "x64" },
  { suffix: "win32-arm64", os: "win32", cpu: "arm64" },
];

const optionalDependencies = Object.fromEntries(
  platforms.map(({ suffix }) => [`@mohsen-azimi/tsz-${suffix}`, version]),
);

const commonMetadata = {
  license: "Apache-2.0",
  author: "Mohsen Azimi <mohsen@users.noreply.github.com>",
  repository: {
    type: "git",
    url: "git+https://github.com/tsz-org/tsz.git",
  },
};

const mainPackage = {
  name: "@mohsen-azimi/tsz",
  version,
  private: true,
  description: "Private R0 TSZ rewrite package for native process-contract validation",
  ...commonMetadata,
  keywords: ["typescript", "compiler", "tsz", "tsc"],
  bin: {
    tsz: "bin/tsz.js",
    "tsz-server": "bin/tsz-server.js",
  },
  optionalDependencies,
  files: ["bin/", "lib-assets/", "LICENSE.txt"],
};
fs.mkdirSync(mainPkg, { recursive: true });
fs.writeFileSync(path.join(mainPkg, "package.json"), JSON.stringify(mainPackage, null, 2) + "\n");

function platformSuffixExpression() {
  return `function platformSuffix() {
  const platform = process.platform;
  const arch = process.arch;
  if (platform === "darwin" && arch === "arm64") return "darwin-arm64";
  if (platform === "darwin" && arch === "x64") return "darwin-x64";
  if (platform === "linux" && arch === "x64") return "linux-x64";
  if (platform === "linux" && arch === "arm64") return "linux-arm64";
  if (platform === "win32" && arch === "x64") return "win32-x64";
  if (platform === "win32" && arch === "arm64") return "win32-arm64";
  return null;
}`;
}

function launcher(binName) {
  return `#!/usr/bin/env node
const { spawnSync } = require("node:child_process");

${platformSuffixExpression()}

const suffix = platformSuffix();
if (!suffix) {
  console.error(\`${binName} does not ship a native binary for \${process.platform}-\${process.arch}\`);
  process.exit(1);
}

const exe = process.platform === "win32" ? "${binName}.exe" : "${binName}";
let binary;
try {
  binary = require.resolve(\`@mohsen-azimi/tsz-\${suffix}/bin/\${exe}\`);
} catch {
  console.error(\`Missing native package @mohsen-azimi/tsz-\${suffix}\`);
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), {
  cwd: process.cwd(),
  env: process.env,
  stdio: "inherit",
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

if (typeof result.status === "number") {
  process.exit(result.status);
}

process.exit(1);
`;
}

fs.mkdirSync(path.join(mainPkg, "bin"), { recursive: true });
fs.writeFileSync(path.join(mainPkg, "bin", "tsz.js"), launcher("tsz"));
fs.writeFileSync(path.join(mainPkg, "bin", "tsz-server.js"), launcher("tsz-server"));

for (const { suffix, os, cpu } of platforms) {
  const pkgDir = path.join(npmDir, "@mohsen-azimi", `tsz-${suffix}`);
  fs.mkdirSync(path.join(pkgDir, "bin"), { recursive: true });
  const pkg = {
    name: `@mohsen-azimi/tsz-${suffix}`,
    version,
    private: true,
    description: `Private R0 native TSZ binaries for ${suffix}`,
    ...commonMetadata,
    os: [os],
    cpu: [cpu],
    files: ["bin/", "LICENSE.txt"],
  };
  fs.writeFileSync(path.join(pkgDir, "package.json"), JSON.stringify(pkg, null, 2) + "\n");
}
NODE

# Copy LICENSE
cp "$PROJECT_ROOT/LICENSE.txt" "$MAIN_PKG/LICENSE.txt"
for entry in "${PLATFORMS[@]}"; do
  read -r npm_suffix _ <<< "$entry"
  platform_pkg="$NPM_DIR/@mohsen-azimi/tsz-$npm_suffix"
  mkdir -p "$platform_pkg"
  cp "$PROJECT_ROOT/LICENSE.txt" "$platform_pkg/LICENSE.txt"
done

# Bundle TypeScript lib files
LIB_ASSETS="$PROJECT_ROOT/crates/tsz-core/data/lib"
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

# ─── Step 3: Assemble try-tsz package ────────────────────────────────────────
echo ""
echo "==> Assembling try-tsz package..."

mkdir -p "$TRY_PKG/bin"
TRY_TSZ_VERSION="$CARGO_VERSION" TRY_TSZ_PKG_FILE="$TRY_PKG/package.json" NPM_DIR="$NPM_DIR" node - <<'NODE'
const fs = require("fs");
const version = process.env.TRY_TSZ_VERSION;
const pkgFile = process.env.TRY_TSZ_PKG_FILE;
const platforms = [
  { suffix: "darwin-arm64", os: "darwin", cpu: "arm64" },
  { suffix: "darwin-x64", os: "darwin", cpu: "x64" },
  { suffix: "linux-x64", os: "linux", cpu: "x64" },
  { suffix: "linux-arm64", os: "linux", cpu: "arm64" },
  { suffix: "win32-x64", os: "win32", cpu: "x64" },
  { suffix: "win32-arm64", os: "win32", cpu: "arm64" },
];
const optionalDependencies = Object.fromEntries(
  platforms.map(({ suffix }) => [`@mohsen-azimi/try-tsz-${suffix}`, version]),
);
const pkg = {
  name: "try-tsz",
  version,
  private: true,
  description: "Private R0 TSZ rewrite oracle-comparison package",
  license: "Apache-2.0",
  author: "Mohsen Azimi <mohsen@users.noreply.github.com>",
  repository: {
    type: "git",
    url: "git+https://github.com/tsz-org/tsz.git",
  },
  keywords: ["typescript", "compiler", "tsz", "tsc"],
  bin: {
    "try-tsz": "bin/try-tsz.js",
  },
  dependencies: {
    "jsonc-parser": "3.3.1",
    typescript: "7.0.2",
  },
  optionalDependencies,
  files: ["bin/", "LICENSE.txt"],
};
fs.writeFileSync(pkgFile, JSON.stringify(pkg, null, 2) + "\n");

const commonMetadata = {
  license: "Apache-2.0",
  author: "Mohsen Azimi <mohsen@users.noreply.github.com>",
  repository: {
    type: "git",
    url: "git+https://github.com/tsz-org/tsz.git",
  },
};

for (const { suffix, os, cpu } of platforms) {
  const pkgDir = `${process.env.NPM_DIR}/@mohsen-azimi/try-tsz-${suffix}`;
  fs.mkdirSync(`${pkgDir}/bin`, { recursive: true });
  const nativePkg = {
    name: `@mohsen-azimi/try-tsz-${suffix}`,
    version,
    private: true,
    description: `Private R0 native try-tsz binary for ${suffix}`,
    ...commonMetadata,
    os: [os],
    cpu: [cpu],
    files: ["bin/", "LICENSE.txt"],
  };
  fs.writeFileSync(`${pkgDir}/package.json`, JSON.stringify(nativePkg, null, 2) + "\n");
}
NODE

cat > "$TRY_PKG/bin/try-tsz.js" <<'NODE'
#!/usr/bin/env node
const { spawnSync } = require("node:child_process");

function platformSuffix() {
  const platform = process.platform;
  const arch = process.arch;
  if (platform === "darwin" && arch === "arm64") return "darwin-arm64";
  if (platform === "darwin" && arch === "x64") return "darwin-x64";
  if (platform === "linux" && arch === "x64") return "linux-x64";
  if (platform === "linux" && arch === "arm64") return "linux-arm64";
  if (platform === "win32" && arch === "x64") return "win32-x64";
  if (platform === "win32" && arch === "arm64") return "win32-arm64";
  return null;
}

const suffix = platformSuffix();
if (!suffix) {
  console.error(`try-tsz does not ship a native binary for ${process.platform}-${process.arch}`);
  process.exit(1);
}

const exe = process.platform === "win32" ? "try-tsz.exe" : "try-tsz";
let binary;
try {
  binary = require.resolve(`@mohsen-azimi/try-tsz-${suffix}/bin/${exe}`);
} catch {
  console.error(`Missing try-tsz native package @mohsen-azimi/try-tsz-${suffix}`);
  process.exit(1);
}

let typescriptPackageJson;
try {
  typescriptPackageJson = require.resolve("typescript/package.json");
} catch {
  console.error("Missing try-tsz TypeScript oracle dependency");
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), {
  cwd: process.cwd(),
  env: {
    ...process.env,
    TRY_TSZ_TYPESCRIPT_PACKAGE_JSON: typescriptPackageJson,
  },
  stdio: "inherit",
});

if (result.error) {
  console.error(result.error.message);
  process.exit(1);
}

if (typeof result.status === "number") {
  process.exit(result.status);
}

process.exit(1);
NODE

chmod +x "$TRY_PKG/bin/try-tsz.js"
cp "$PROJECT_ROOT/LICENSE.txt" "$TRY_PKG/LICENSE.txt"
for entry in "${PLATFORMS[@]}"; do
  read -r npm_suffix _ <<< "$entry"
  try_platform_pkg="$NPM_DIR/@mohsen-azimi/try-tsz-$npm_suffix"
  mkdir -p "$try_platform_pkg"
  cp "$PROJECT_ROOT/LICENSE.txt" "$try_platform_pkg/LICENSE.txt"
done

# GitHub artifact upload/download does not preserve executable bits reliably.
# Restore them during package assembly so npm packs native binaries as runnable.
find "$NPM_DIR/@mohsen-azimi" -path "*/bin/*" -type f -exec chmod +x {} +

echo ""
echo "==> Build complete!"
echo "    Main package: $MAIN_PKG"
echo "    Try package:  $TRY_PKG"
for platform_suffix in "${BUILD_PLATFORMS[@]}"; do
  echo "    Platform:     $NPM_DIR/@mohsen-azimi/tsz-$platform_suffix"
  echo "    Try platform: $NPM_DIR/@mohsen-azimi/try-tsz-$platform_suffix"
done
echo ""
echo "To test locally:"
echo "  cd $MAIN_PKG && npm link"
echo "  tsz --noEmit"
echo ""
echo "Publication is intentionally disabled during the clean-slate rewrite."
