# fix(checker): suppress false TS2456 for typeof through TYPE_LITERAL deferral

- **Date**: 2026-04-26
- **Branch**: `fix/checker-typeof-typeliteral-no-circular`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (conformance)

## Intent

`type T27 = typeof a27; declare var a27: { prop: number } | { prop: T27 };`
should NOT emit TS2456. tsc treats TYPE_LITERAL property types as lazily
resolved, so a self-reference inside `{ prop: T27 }` is structurally deferred.

We currently walk the typeof-target's annotation AST in
`ast_finds_resolution_chain_alias` and descend into all children, including
TYPE_LITERAL/MAPPED_TYPE/FUNCTION_TYPE/CONSTRUCTOR_TYPE — these are the
exact deferral wrappers tsc uses to terminate eager type construction.

Fix: skip descent into structurally deferred wrapper kinds in the AST walk
used for typeof annotation circularity detection.

## Files Touched

- `crates/tsz-checker/src/state/type_analysis/computed_helpers.rs` (~10 LOC)
- `crates/tsz-checker/tests/...` regression test (~30 LOC)

## Verification

- `./scripts/conformance/conformance.sh run --filter unionTypeWithRecursiveSubtypeReduction3`
- `./scripts/conformance/conformance.sh run --filter directDependenceBetweenTypeAliases`
- `./scripts/conformance/conformance.sh run --filter circularTypeofWithVarOrFunc`
- `cargo nextest run -p tsz-checker --lib`

## Conformance Impact

- +1 test (`unionTypeWithRecursiveSubtypeReduction3.ts`) flips PASS by removing
  spurious TS2456. Note the secondary TS2322 fingerprint diff is on display
  text and may also flip with the underlying type now resolving.
