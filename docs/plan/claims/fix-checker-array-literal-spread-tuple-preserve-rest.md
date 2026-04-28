# Preserve rest flag when spreading a variadic tuple in tuple context

- **Date**: 2026-04-27
- **Branch**: `fix/checker-array-literal-spread-tuple-preserve-rest`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

`crates/tsz-checker/src/types/computation/array_literal.rs:631` hardcodes
`rest: false` when expanding a spread tuple's elements into the result tuple.
For a source tuple with a trailing rest element (e.g.
`[string, boolean, ...boolean[]]`), this collapses the variadic tail into a
single fixed `boolean[]` element, producing `[number, string, boolean, boolean[]]`
instead of `[number, string, boolean, ...boolean[]]`.

The bug surfaces in two ways: (1) garbled diagnostic display (`boolean[]`
vs `...boolean[]` in TS2322 messages), and (2) spurious TS2322 emissions
when the target tuple's own rest signature would accept the source's
variadic tail.

Fix: forward `elem.rest` from the source tuple element instead of hardcoding
`false`.

## Files Touched

- `crates/tsz-checker/src/types/computation/array_literal.rs` (~1 LOC change
  + comment expansion)
- `crates/tsz-checker/tests/spread_rest_tests.rs` (+2 regression tests)

## Verification

- `cargo nextest run -p tsz-checker -E 'test(spread) | test(tuple) | test(rest)'`
  → 368 pass.
- `./scripts/conformance/conformance.sh run --filter "spliceTuples"` → **1/1 pass** (was 0/1).
- `./scripts/conformance/conformance.sh run --filter "tuple"` → unchanged failure set
  (no regressions; spliceTuples flipped to PASS).
- `./scripts/conformance/conformance.sh run --filter "spread"` → unchanged failure set
  (no regressions).
