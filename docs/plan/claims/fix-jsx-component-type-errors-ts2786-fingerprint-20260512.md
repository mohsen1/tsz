# Claim: jsxComponentTypeErrors TS2786 fingerprint

Status: claim
Owner: Codex
Branch: fix/jsx-component-type-errors-ts2786-fingerprint-20260512
PR: TBD

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
