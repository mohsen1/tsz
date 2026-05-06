# fix(checker): align overload1 TS2769 anchor

- **Date**: 2026-05-06
- **Branch**: `fix/checker-overload1-ts2769-anchor`
- **PR**: #3857
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

Reduce the fingerprint-only conformance failure in
`TypeScript/tests/cases/compiler/overload1.ts`. Both tsc and tsz emit
`TS2322`, `TS2554`, and `TS2769`, but the ambiguous overload call
`z=x.h(2,2)` anchors `TS2769` differently. tsc reports the diagnostic at the
overloaded property token (`h`), while tsz reported it at the first argument.

## Files Touched

- `docs/plan/claims/fix-checker-overload1-ts2769-anchor.md`
- `crates/tsz-checker/src/error_reporter/call_errors/error_emission.rs`
- `crates/tsz-checker/src/error_reporter/call_errors_overload_tests.rs`

## Verification

- `cargo fmt --check`
- `CARGO_TARGET_DIR=.target cargo check -p tsz-checker`
- `CARGO_TARGET_DIR=.target cargo nextest run -p tsz-checker ts2769_property_call_multi_arg_mismatch_anchors_property_token ts2769_assignment_rhs_overload_mismatch_anchors_argument`
- `./scripts/conformance/conformance.sh run --filter "overload1" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run` (`12460/12582 passed`, refreshed baseline)
