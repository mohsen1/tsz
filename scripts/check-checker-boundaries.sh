#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

# Guardrail: checker should not directly inspect type internals via lookup()
# outside query boundaries and test-only scaffolding.
PATTERN='\.lookup\('

HITS="$(rg -n "$PATTERN" crates/tsz-checker/src \
  --glob '!**/query_boundaries/**' \
  --glob '!**/tests/**' || true)"

if [[ -n "$HITS" ]]; then
  echo "Checker boundary guardrail violation: direct lookup() found outside allowed zones:"
  echo "$HITS"
  echo ""
  echo "Move this inspection behind crate::query_boundaries and solver type_queries helpers."
  exit 1
fi

echo "Checker boundary guardrail passed."
