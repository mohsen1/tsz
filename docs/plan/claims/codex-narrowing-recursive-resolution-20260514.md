# Claim: Guard recursive narrowing resolution

Status: ready
Branch: codex/narrowing-recursive-resolution-20260514
PR: pending

## Scope
- Prevent stack overflow in narrowing-time type resolution for recursive `keyof` / indexed-access / conditional graphs.
- Preserve unresolved generic/deferred form when a resolution cycle is encountered.

## Validation
- Focused conformance:
  `TSZ_LIB_DIR=/Users/mohsen/code/tsz/.worktrees/fix-export-equals-require-surface-20260509/TypeScript/lib ./scripts/conformance/conformance.sh run --filter intersectionsOfLargeUnions2 --test-dir /Users/mohsen/code/tsz/.worktrees/fix-export-equals-require-surface-20260509/TypeScript/tests/cases --workers 1 --verbose`
  Result: `FINAL RESULTS: 1/1 passed (100.0%)`; crashed 0; timeout 0; fingerprint-only 0.
- Integration conformance with parse-recovery and conditional-alias-display fixes:
  `TSZ_LIB_DIR=/Users/mohsen/code/tsz/.worktrees/fix-export-equals-require-surface-20260509/TypeScript/lib ./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/.worktrees/fix-export-equals-require-surface-20260509/TypeScript/tests/cases --workers 8`
  Result: `FINAL RESULTS: 12585/12585 passed (100.0%)`; skipped 0; known failures 0; crashed 0; timeout 0; fingerprint-only 0.
