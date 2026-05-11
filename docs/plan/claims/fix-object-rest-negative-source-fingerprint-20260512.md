# Claim: objectRestNegative rest assignment source fingerprint

Status: claim
Owner: Codex
Branch: fix/object-rest-negative-source-fingerprint-20260512
PR: TBD

## Target

Close the current fingerprint-only mismatch in `TypeScript/tests/cases/conformance/types/rest/objectRestNegative.ts`.

Current baseline on `main`:

```text
scripts/safe-run.sh ./scripts/conformance/conformance.sh run --filter objectRestNegative --verbose
FINAL RESULTS: 0/1 passed
Fingerprint-only: 1
missing: TS2322 test.ts:6:10 Type '{ a: number; }' is not assignable to type '{ a: string; }'.
extra:   TS2322 test.ts:6:10 Type '{ a: string; }' is not assignable to type '{ a: string; }'.
```

## Plan

Fix the checker path for object-rest assignment so the TS2322 source type reflects the actual rest value from the RHS (`{ a: number; }`) instead of echoing the annotated target type (`{ a: string; }`). Add focused regression coverage and rerun the targeted conformance case.
