# Claim: jsxComponentTypeErrors TS2786 fingerprint

Status: ready
Owner: Codex
Branch: fix/jsx-component-type-errors-ts2786-fingerprint-20260512
PR: #5660

## Target

Close the current fingerprint-only mismatch in `TypeScript/tests/cases/compiler/jsxComponentTypeErrors.tsx`.

Current baseline on `main`:

```text
scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter jsxComponentTypeErrors --verbose
FINAL RESULTS: 0/1 passed
Fingerprint-only: 1
missing: TS2786 test.tsx:28:16 'MixedComponent' cannot be used as a JSX component.
```

## Plan

Fix the JSX component validity diagnostic path so the TS2786 fingerprint for `<MixedComponent />` matches tsc's anchor/message while preserving the single expected TS2786 code.

## Result

Implemented union JSX component return validation after recovered props resolution. Validation:

```text
cargo test -p tsz-checker jsx_union_of_invalid_function_and_class_component_emits_ts2786
scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter jsxComponentTypeErrors --verbose
FINAL RESULTS: 1/1 passed
Fingerprint-only: 0
cargo fmt --all --check
git diff --check
```
