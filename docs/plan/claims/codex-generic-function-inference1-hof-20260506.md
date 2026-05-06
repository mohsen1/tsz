# Claim: Suppress spurious higher-order generic inference TS2322

## Target

`TypeScript/tests/cases/compiler/genericFunctionInference1.ts`

Current `origin/main` emits two extra TS2322 diagnostics for the same
higher-order generic pipeline pattern:

```text
extra: TS2322 test.ts:33:7 Type '(...args: [x: { value: T; }]) => { value: T; }' is not assignable to type '<T extends { value: T; }>(x: T) => T'.
extra: TS2322 test.ts:34:7 Type '(...args: [x: { value: T; }]) => { value: T; }' is not assignable to type '<T extends { value: T; }>(x: T) => T'.
```

## Plan

Reproduce the fixture locally, identify why higher-order inference loses the
generic constraint on the composed `pipe(foo)` result, then add a focused
regression that removes these false assignment diagnostics without weakening
ordinary function assignment checks.

## Implementation

When a higher-order call produces a non-generic function shape that still
contains nonlocal type parameters, subtype checking now hoists matching
nonlocal parameters before comparing against a generic target signature. If the
source parameter was widened to the target constraint object, the check rewrites
that exact constraint-shaped position back to the hoisted type parameter before
the normal generic-function comparison.

Added a focused checker regression covering both `pipe(foo)` and
`pipe(foo, foo)` assignment to `<T extends { value: T }>(x: T) => T`.

## Verification

- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test generic_call_inference_tests pipe_preserves_self_constrained_generic_function_result -- --exact --nocapture`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo test -p tsz-checker --test generic_call_inference_tests -- --nocapture`
- `CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 cargo build --profile dist-fast -p tsz-cli -p tsz-conformance --target-dir .target-dist`
- `.target-dist/dist-fast/tsz-conformance --test-dir <tmp> --cache-file scripts/conformance/tsc-cache-full.json --tsz-binary .target-dist/dist-fast/tsz --workers 1 --verbose --print-fingerprints --print-test-files --no-batch --timeout 60`

Filtered conformance result: `FINAL RESULTS: 1/1 passed (100.0%)`.

## Status

Implemented and verified on 2026-05-06.
