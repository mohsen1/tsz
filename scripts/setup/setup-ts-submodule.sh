#!/bin/bash
# Backward-compatible entrypoint for the standalone TypeScript corpus checkout.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/setup/setup-ts-submodule.sh

Creates or repairs the standalone, sparse TypeScript/ corpus checkout at the
exact repository pin. The historical filename is retained for existing local
workflows; this command does not create or modify a Git submodule.
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1 (try --help)" >&2
      exit 2
      ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/reset-ts-submodule.sh" --sparse
