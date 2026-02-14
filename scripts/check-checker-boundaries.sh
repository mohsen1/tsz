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
  --glob '!**/tests/**' || true)"

if [[ -n "$TYPEKEY_LEAK_HITS" ]]; then
  echo "Checker boundary guardrail violation: direct TypeKey import/intern usage found:"
  echo "$TYPEKEY_LEAK_HITS"
  echo ""
  echo "Use solver constructor/query APIs instead of importing or interning TypeKey in checker code."
  exit 1
fi

echo "Checker TypeKey import/intern guardrail passed."

# Guardrail: checker must not import solver internal module paths directly.
#
# Checker should consume only public tsz_solver re-exports, not tsz_solver::types::...
SOLVER_INTERNAL_IMPORT_HITS="$(rg -n "tsz_solver::types::" crates/tsz-checker/src \
  --glob '!**/tests/**' || true)"

if [[ -n "$SOLVER_INTERNAL_IMPORT_HITS" ]]; then
  echo "Checker boundary guardrail violation: direct solver internal module imports found:"
  echo "$SOLVER_INTERNAL_IMPORT_HITS"
  echo ""
  echo "Use public tsz_solver::* exports instead of tsz_solver::types::* paths in checker code."
  exit 1
fi

echo "Checker solver-internal import guardrail passed."

# Guardrail: checker must construct semantic types through solver constructor APIs,
# not by calling the raw interner directly.
RAW_INTERN_HITS="$(rg -n "\\.intern\\(" crates/tsz-checker/src \
  --glob '!**/tests/**' || true)"

if [[ -n "$RAW_INTERN_HITS" ]]; then
  echo "Checker boundary guardrail violation: raw interner access found in checker code:"
  echo "$RAW_INTERN_HITS"
  echo ""
  echo "Use solver constructor APIs (types.array/union/intersection/lazy/etc.) instead of .intern(...)."
  exit 1
fi

echo "Checker raw interner access guardrail passed."

# Guardrail: solver crate dependency direction freeze.
#
# Milestone 0 target is "solver must not import parser/checker crates".
# Current known legacy exception: crates/tsz-solver/src/lower.rs.
# This guard prevents architectural drift by failing on any new site.
SOLVER_DEP_HITS="$(rg -n "\btsz_parser::\b|\btsz_checker::\b" crates/tsz-solver/src \
  --glob '!**/tests/**' || true)"
SOLVER_DEP_NEW_HITS="$(printf '%s\n' "$SOLVER_DEP_HITS" | rg -v "^crates/tsz-solver/src/lower.rs:" || true)"

if [[ -n "$SOLVER_DEP_NEW_HITS" ]]; then
  echo "Architecture guardrail violation: new solver imports parser/checker crates found:"
  echo "$SOLVER_DEP_NEW_HITS"
  echo ""
  echo "Keep parser/checker dependencies quarantined to the legacy lower.rs path while migration is in progress."
  exit 1
fi

echo "Solver dependency-direction freeze guardrail passed (legacy lower.rs exception only)."
