# Fix contextual signature instantiation diagnostics

- **Date**: 2026-05-01
- **Branch**: `codex/fix-contextual-signature-instantiation`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the remaining `contextualSignatureInstantiation.ts` conformance miss, now
narrowed on current `main` to one missing TS2345 plus the downstream TS2403.
The target is contextual instantiation of a generic function argument against a
non-generic callback parameter, where `foo(g)` should reject the disjoint
`number`/`string` parameter pair instead of accepting the call with `unknown`.

## Files Touched

- `crates/tsz-solver/src/operations/` (expected)
- `crates/tsz-checker/tests/` (expected regression coverage)

## Verification

- Pending focused checker tests
- Pending targeted conformance run for `contextualSignatureInstantiation`
