# [WIP] fix(parser): align Annex B regex diagnostic positions

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-regex-annexb-20260505`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the quick-picked fingerprint-only mismatch in
`regularExpressionAnnexB.ts`. The diagnostic code set already matches tsc, but
regex parser diagnostics for incomplete quantifiers, nothing-to-repeat errors,
and invalid escapes are reported at different source positions than tsc.

## Files Touched

- `crates/tsz-parser/src/parser/state_expressions_literals_regex.rs`
- parser regression tests TBD

## Verification

- `PYTHONUNBUFFERED=1 scripts/session/quick-pick.sh --run` selected and
  reproduced `TypeScript/tests/cases/compiler/regularExpressionAnnexB.ts` as a
  fingerprint-only failure.
