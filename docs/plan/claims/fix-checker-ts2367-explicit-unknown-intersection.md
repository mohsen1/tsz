# fix(checker): emit TS2367 for explicit unknown intersection comparisons

- **Date**: 2026-04-27
- **Time**: 2026-04-27 01:35:05 UTC
- **Branch**: `codex/conformance-ts2367-unknown-controlflow`
- **PR**: pending
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Fix the missing TS2367 diagnostics in
`TypeScript/tests/cases/conformance/types/unknown/unknownControlFlow.ts` for
comparisons like:

```ts
function f<T extends unknown>(value: T & ({} | null)) {
    if (value === 42) {}
}

function g<T extends {} | undefined>(value: T & ({} | null)) {
    if (value === 42) {}
}
```

tsc reports these as no-overlap comparisons between `T` and `number`. tsz
currently treats the explicit `unknown`/undefined-bearing type-parameter
constraint as overlap-permissive and misses TS2367.

This is distinct from the active decorator, array `toLocaleString`, type-display,
index-signature, JSX, overload, namespace/import, and parser-recovery lanes.

## Verification Plan

CI only per request. Add focused checker regression coverage and rely on PR CI
for build, unit, and conformance validation.
