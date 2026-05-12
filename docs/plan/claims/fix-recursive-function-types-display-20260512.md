# Claim: recursiveFunctionTypes display fingerprints

Status: ready
Owner: Codex
Branch: fix/recursive-function-types-display-20260512
PR: #5663

## Target

Close the current fingerprint-only mismatch in `TypeScript/tests/cases/compiler/recursiveFunctionTypes.ts`.

Current baseline on `main` after PR #5658 merge:

```text
scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter recursiveFunctionTypes --verbose
FINAL RESULTS: 1/2 passed
Fingerprint-only: 1
missing: TS2322 test.ts:25:1 Type 'number' is not assignable to type '() => ...'.
missing: TS2345 test.ts:34:4 Argument of type 'string' is not assignable to parameter of type '{ (): typeof f6; (a: typeof f6): () => number; }'.
extra: TS2322 test.ts:25:1 Type 'number' is not assignable to type '() => () => typeof f4'.
extra: TS2345 test.ts:34:4 Argument of type 'string' is not assignable to parameter of type 'typeof f6'.
```

## Plan

Adjust recursive function/callable type diagnostic display so recursive return chains collapse to the tsc ellipsis surface and overloaded recursive function types use their callable object surface where expected. Add focused regression coverage and rerun the target conformance filter.

## Result

Implemented diagnostic display handling for recursive `typeof` function returns and overloaded recursive `typeof` call parameters.

Validation:

```text
scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter recursiveFunctionTypes --verbose
FINAL RESULTS: 2/2 passed (100.0%)
Fingerprint-only: 0
cargo test -p tsz-checker recursive_
cargo fmt --all --check
git diff --check
```
