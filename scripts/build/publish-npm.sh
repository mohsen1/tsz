#!/usr/bin/env bash
set -euo pipefail

echo "npm publication is unavailable during the clean-slate rewrite." >&2
echo "Use scripts/build/build-npm-packages.sh --local for private R0 package inspection." >&2
exit 1
