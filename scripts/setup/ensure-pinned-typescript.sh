#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
VERSIONS_FILE="$ROOT_DIR/scripts/conformance/typescript-versions.json"

usage() {
    local stream="${1:-1}"
    cat >&"$stream" <<'EOF'
Usage: ./scripts/setup/ensure-pinned-typescript.sh <project_dir>

Ensures a project directory has a TypeScript installation matching the
currently pinned version in scripts/conformance/typescript-versions.json.
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

if [ $# -ne 1 ]; then
    usage 2
    exit 2
fi

PROJECT_DIR="$1"
if [[ "$PROJECT_DIR" == -* ]]; then
    echo "Unknown option: $PROJECT_DIR (try --help)" >&2
    exit 2
fi

if [ ! -d "$PROJECT_DIR" ]; then
    echo "ERROR: Project directory not found: $PROJECT_DIR" >&2
    exit 2
fi

if [ ! -f "$VERSIONS_FILE" ]; then
    echo "ERROR: Missing versions file: $VERSIONS_FILE" >&2
    exit 1
fi

if ! command -v node >/dev/null 2>&1; then
    echo "ERROR: node is required" >&2
    exit 1
fi

if ! command -v npm >/dev/null 2>&1; then
    echo "ERROR: npm is required" >&2
    exit 1
fi

PINNED_VERSION="$(node -e "const fs = require('fs'); const file = process.argv[1]; const cfg = JSON.parse(fs.readFileSync(file, 'utf8')); const current = cfg.current || ''; const mapped = current && cfg.mappings && cfg.mappings[current] && cfg.mappings[current].npm; const fallback = cfg.default && cfg.default.npm; process.stdout.write(mapped || fallback || '');" "$VERSIONS_FILE")"

if [ -z "$PINNED_VERSION" ]; then
    echo "ERROR: Could not resolve pinned TypeScript version from $VERSIONS_FILE" >&2
    exit 1
fi

PACKAGE_JSON="$PROJECT_DIR/node_modules/typescript/package.json"
LIB_RESOLVER="$ROOT_DIR/scripts/setup/resolve-typescript-lib-dir.mjs"
INSTALL_TS=false

if [ ! -d "$PROJECT_DIR/node_modules" ]; then
    INSTALL_TS=true
fi

CURRENT_VERSION=""
if [ -f "$PACKAGE_JSON" ]; then
    CURRENT_VERSION="$(node -e "const fs = require('fs'); const file = process.argv[1]; try { const pkg = JSON.parse(fs.readFileSync(file, 'utf8')); process.stdout.write(pkg.version || ''); } catch { process.stdout.write(''); }" "$PACKAGE_JSON")"
fi

if [ "$CURRENT_VERSION" != "$PINNED_VERSION" ]; then
    INSTALL_TS=true
fi

if [ "$INSTALL_TS" = true ]; then
    if [ -f "$PROJECT_DIR/package.json" ]; then
        if [ ! -d "$PROJECT_DIR/node_modules" ] || [ ! -d "$PROJECT_DIR/node_modules/typescript" ]; then
            echo "Installing npm dependencies for $PROJECT_DIR ..."
            (cd "$PROJECT_DIR" && npm install --silent --no-audit --no-fund --no-package-lock)
        fi

        echo "Installing pinned TypeScript $PINNED_VERSION into $PROJECT_DIR ..."
        # Always use --legacy-peer-deps to prevent npm from removing
        # existing packages (like @types/chai) during peer resolution
        (cd "$PROJECT_DIR" && npm install --silent --no-save --no-audit --no-fund --no-package-lock --ignore-scripts --legacy-peer-deps "typescript@${PINNED_VERSION}")
    fi

    if [ -f "$PACKAGE_JSON" ]; then
        CURRENT_VERSION="$(node -e "const fs = require('fs'); const file = process.argv[1]; try { const pkg = JSON.parse(fs.readFileSync(file, 'utf8')); process.stdout.write(pkg.version || ''); } catch { process.stdout.write(''); }" "$PACKAGE_JSON")"
    fi

    if [ "$CURRENT_VERSION" != "$PINNED_VERSION" ]; then
        echo "ERROR: Failed to install pinned TypeScript version ($PINNED_VERSION) in $PROJECT_DIR" >&2
        echo "Installed version: ${CURRENT_VERSION:-<none>}" >&2
        exit 1
    fi
fi

PLATFORM_PACKAGE="$(node -e "process.stdout.write('@typescript/typescript-' + process.platform + '-' + process.arch)")"
EXPECTED_PLATFORM_VERSION="$(node -e "const fs = require('fs'); const pkg = JSON.parse(fs.readFileSync(process.argv[1], 'utf8')); process.stdout.write((pkg.optionalDependencies && pkg.optionalDependencies[process.argv[2]]) || '');" "$PACKAGE_JSON" "$PLATFORM_PACKAGE")"

if [ -n "$EXPECTED_PLATFORM_VERSION" ]; then
    if [ "$EXPECTED_PLATFORM_VERSION" != "$PINNED_VERSION" ]; then
        echo "ERROR: $PLATFORM_PACKAGE is pinned to $EXPECTED_PLATFORM_VERSION by the TypeScript wrapper, expected $PINNED_VERSION" >&2
        exit 1
    fi

    PLATFORM_PACKAGE_JSON="$PROJECT_DIR/node_modules/$PLATFORM_PACKAGE/package.json"
    CURRENT_PLATFORM_VERSION=""
    if [ -f "$PLATFORM_PACKAGE_JSON" ]; then
        CURRENT_PLATFORM_VERSION="$(node -e "const fs = require('fs'); const file = process.argv[1]; try { const pkg = JSON.parse(fs.readFileSync(file, 'utf8')); process.stdout.write(pkg.version || ''); } catch { process.stdout.write(''); }" "$PLATFORM_PACKAGE_JSON")"
    fi

    if [ "$CURRENT_PLATFORM_VERSION" != "$EXPECTED_PLATFORM_VERSION" ]; then
        echo "Installing pinned TypeScript platform package $PLATFORM_PACKAGE@$EXPECTED_PLATFORM_VERSION into $PROJECT_DIR ..."
        (cd "$PROJECT_DIR" && npm install --silent --no-save --no-audit --no-fund --no-package-lock --ignore-scripts --legacy-peer-deps --include=optional "${PLATFORM_PACKAGE}@${EXPECTED_PLATFORM_VERSION}")

        CURRENT_PLATFORM_VERSION=""
        if [ -f "$PLATFORM_PACKAGE_JSON" ]; then
            CURRENT_PLATFORM_VERSION="$(node -e "const fs = require('fs'); const file = process.argv[1]; try { const pkg = JSON.parse(fs.readFileSync(file, 'utf8')); process.stdout.write(pkg.version || ''); } catch { process.stdout.write(''); }" "$PLATFORM_PACKAGE_JSON")"
        fi
        if [ "$CURRENT_PLATFORM_VERSION" != "$EXPECTED_PLATFORM_VERSION" ]; then
            echo "ERROR: Failed to install $PLATFORM_PACKAGE@$EXPECTED_PLATFORM_VERSION in $PROJECT_DIR" >&2
            echo "Installed platform version: ${CURRENT_PLATFORM_VERSION:-<none>}" >&2
            exit 1
        fi
    fi
elif [[ "$PINNED_VERSION" == 7.* ]]; then
    echo "ERROR: TypeScript $PINNED_VERSION does not declare the required optional package $PLATFORM_PACKAGE" >&2
    exit 1
fi

TSC_JS="$PROJECT_DIR/node_modules/typescript/lib/tsc.js"
if [ ! -f "$TSC_JS" ]; then
    echo "ERROR: Pinned TypeScript CLI launcher not found: $TSC_JS" >&2
    exit 1
fi

TSC_VERSION="$(node "$TSC_JS" --version 2>/dev/null | sed -n 's/^Version //p' | head -1)"
if [ "$TSC_VERSION" != "$PINNED_VERSION" ]; then
    echo "ERROR: Pinned TypeScript CLI reports ${TSC_VERSION:-<none>}, expected $PINNED_VERSION" >&2
    exit 1
fi

if ! LIB_DIR="$(node "$LIB_RESOLVER" "$PACKAGE_JSON")"; then
    echo "ERROR: Pinned TypeScript standard libraries are unavailable" >&2
    exit 1
fi

echo "$PROJECT_DIR TypeScript version: $PINNED_VERSION"
echo "$PROJECT_DIR TypeScript lib dir: $LIB_DIR"
exit 0
