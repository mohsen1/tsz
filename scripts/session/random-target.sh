#!/usr/bin/env bash
# =============================================================================
# random-target.sh — One-line random conformance target picker
# =============================================================================
#
# Prints a single random failing conformance target as a one-line summary,
# suitable for shell pipelines and quick "what should I work on" prompts.
#
# Output format:
#   <filter-name>\t<category>\t<expected-codes>\t<actual-codes>\t<path>
#
# Usage:
#   scripts/session/random-target.sh                   # any failure
#   scripts/session/random-target.sh --code TS2322     # filter by error code
#   scripts/session/random-target.sh --seed 42         # reproducible pick
#   scripts/session/random-target.sh --filter          # print just filter name
#
# For a richer human-readable display, use scripts/session/quick-pick.sh.
# Selection logic lives in scripts/session/pick.py (`one` subcommand).
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/pick.py" one "$@"
