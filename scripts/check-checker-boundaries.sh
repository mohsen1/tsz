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

# Guardrail: checker should not pattern-match TypeKey internals directly
# outside query boundaries and tests.
INSPECT_HITS="$(rg -n "^\s*(match|if let|if matches!|matches!\().*TypeKey::" crates/tsz-checker/src \
  --glob '!**/query_boundaries/**' \
  --glob '!**/tests/**' || true)"

if [[ -n "$INSPECT_HITS" ]]; then
  echo "Checker boundary guardrail violation: direct TypeKey inspection found:"
  echo "$INSPECT_HITS"
  echo ""
  echo "Move this logic into solver type_queries + checker query_boundaries wrappers."
  exit 1
fi

echo "Checker TypeKey inspection guardrail passed."

# Guardrail: checker must not import or directly intern TypeKey.
#
# This enforces Milestone 2 boundary sealing at the script/CI level rather than
# relying only on crate-local tests.
TYPEKEY_LEAK_HITS="$(rg -n "(use\\s+tsz_solver::.*TypeKey|intern\\(TypeKey::|intern\\(tsz_solver::TypeKey::)" crates/tsz-checker/src \
  --glob '!**/tests/**' \
  --glob '!**/query_boundaries/**' || true)"

if [[ -n "$TYPEKEY_LEAK_HITS" ]]; then
  echo "Checker boundary guardrail violation: direct TypeKey import/intern usage found:"
  echo "$TYPEKEY_LEAK_HITS"
  echo ""
  echo "Use solver constructor/query APIs instead of importing or interning TypeKey in checker code."
  exit 1
fi

echo "Checker TypeKey import/intern guardrail passed."
