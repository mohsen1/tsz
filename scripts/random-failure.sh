#!/usr/bin/env bash
# =============================================================================
# random-failure.sh — Pick one random conformance failure to work on
# =============================================================================
#
# A top-level convenience entry point for the canonical picker at
# scripts/session/quick-pick.sh. Use this when you just want something to
# work on; no flags needed.
#
# Usage:
#   scripts/random-failure.sh                # any failure
#   scripts/random-failure.sh --code TS2322  # filter by error code
#   scripts/random-failure.sh --seed 42      # reproducible pick
#   scripts/random-failure.sh --run          # also runs it with --verbose
#
# See scripts/session/conformance-agent-prompt.md for the full workflow.
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/session/quick-pick.sh" "$@"
