#!/usr/bin/env bash
# Guard: the repo's shell scripts must run under the Bash that ships with macOS
# (`/bin/bash`, version 3.2). Bash 4+ builtins and parameter expansions silently
# degrade there, collapsing package discovery and running Cargo with empty
# arguments (issue #15440). The scanner lives in lib/bash32_compat_guard.py so
# its pattern definitions do not trip the guard's own `.sh`-only scan; with no
# arguments it scans all of `scripts/`.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

exec python3 scripts/ci/lib/bash32_compat_guard.py "$@"
