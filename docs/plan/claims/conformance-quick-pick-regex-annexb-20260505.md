# [WIP] fix(parser): align Annex B regex diagnostic positions

- **Date**: 2026-05-05
- **Branch**: `conformance/quick-pick-regex-annexb-20260505`
- **PR**: #3392
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the quick-picked fingerprint-only mismatch in
`regularExpressionAnnexB.ts`. The diagnostic code set already matches tsc, but
regex parser diagnostics for incomplete quantifiers, nothing-to-repeat errors,
and invalid escapes are reported at different source positions than tsc.

## Files Touched

- `crates/tsz-parser/src/parser/state_expressions_literals_regex.rs`
- `crates/tsz-parser/tests/parser_improvement_tests.rs`
- `docs/plan/claims/conformance-quick-pick-regex-annexb-20260505.md`

## Verification

- `PYTHONUNBUFFERED=1 scripts/session/quick-pick.sh --run` selected and
  reproduced `TypeScript/tests/cases/compiler/regularExpressionAnnexB.ts` as a
  fingerprint-only failure.
- `cargo fmt --all --check`
- `cargo nextest run -p tsz-parser test_regex_annex_b_diagnostic_positions_match_tsc --failure-output immediate-final --no-fail-fast`
- `cargo nextest run -p tsz-parser regex --failure-output immediate-final --no-fail-fast`
- `CARGO_BUILD_JOBS=1 ./scripts/conformance/conformance.sh run --filter "regularExpressionAnnexB" --workers 1 --verbose`
  - `FINAL RESULTS: 1/1 passed (100.0%)`
  - `Fingerprint-only: 0`
