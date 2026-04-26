# fix(checker): suppress TS2532 on property access when TS2454 fires within the receiver span

- **Date**: 2026-04-26
- **Branch**: `fix/checker-ts2532-suppress-when-ts2454-in-receiver`
- **PR**: #1439
- **Status**: ready
- **Workstream**: 1 (conformance)

## Intent

`unionAndIntersectionInference1.ts` produces a false TS2532 ("Object is
possibly 'undefined'.") on `get(foo).toUpperCase()` where `foo` is an
uninitialized `let foo: Maybe<string>`. tsc emits only TS2454 ("Variable
'foo' is used before being assigned.") and suppresses TS2532 because the
receiver expression contains a definite-assignment failure.

Today the checker's `receiver_has_daa_error` heuristic only matches when the
receiver itself is the DAA-flagged node, or when a TS2454 diagnostic starts
exactly at the receiver's start position. For composite receivers like
`get(foo)` (a `CallExpression` wrapping the failing identifier), neither
condition holds, so TS2532 leaks through.

This change extends `receiver_has_daa_error` to cover any TS2454 diagnostic
whose span lies within the receiver's `[pos, end]` range.

## Files Touched

- `crates/tsz-checker/src/types/property_access_type/resolve.rs` (~20 LOC)
- `crates/tsz-checker/tests/definite_assignment_tests.rs` (+45 LOC test)

## Verification

- `cargo nextest run -p tsz-checker --lib` — 2893 / 2893 pass.
- `cargo nextest run -p tsz-checker -E 'test(definite_assignment) | test(ts2454) | test(ts2532)'` — 81 / 81 pass.
- `./scripts/conformance/conformance.sh run --filter "unionAndIntersectionInference1" --verbose` — TS2532 false positive eliminated.
- Pre-commit (cargo fmt, clippy, wasm32 rustc, architecture guardrails, full nextest 13777) all pass.
