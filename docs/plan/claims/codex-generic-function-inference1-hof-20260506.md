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

## Verification

- Focused Rust regression for the changed higher-order inference path.
- Filtered conformance for `compiler/genericFunctionInference1.ts` with
  `--print-fingerprints`.

## Status

Claimed on 2026-05-06 before implementation.
