#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSIONS_FILE="$ROOT_DIR/scripts/typescript-versions.json"

usage() {
    cat <<'EOF'
Usage: ./scripts/ensure-pinned-typescript.sh <project_dir>

Ensures a project directory has a TypeScript installation matching the
currently pinned version in scripts/typescript-versions.json.
EOF
}

if [ $# -lt 1 ]; then
    usage
    exit 2
fi

PROJECT_DIR="$1"
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
        (cd "$PROJECT_DIR" && npm install --silent --no-save --no-audit --no-fund --no-package-lock --ignore-scripts "typescript@${PINNED_VERSION}")
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

echo "$PROJECT_DIR TypeScript version: $PINNED_VERSION"
exit 0
