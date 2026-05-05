# fix(checker): align TS2345 contextual literal inference fingerprint

- **Date**: 2026-05-05
- **Branch**: `fix-ts2345-contextual-literal-inference`
- **PR**: #2762
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the refreshed random conformance pick
`paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized.ts`, where `tsc`
and `tsz` both emit TS2345 but disagree on the diagnostic fingerprint.
The suspected surface is generic call inference and the display of literal
arguments when a homomorphic mapped type provides lower-priority contextual
inference.

## Files Touched

- `docs/plan/claims/fix-ts2345-contextual-literal-inference.md`
- `crates/tsz-solver/src/operations/constraints/walker.rs`
- `crates/tsz-solver/src/diagnostics/format/mod.rs`
- `crates/tsz-checker/src/error_reporter/core_formatting.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`

## Verification

- `cargo nextest run -p tsz-checker --test generic_call_inference_tests mapped_object_key_inference_is_lower_priority_than_direct_key_argument` - passed.
- `cargo nextest run -p tsz-checker --test generic_call_inference_tests` - passed, 81/81.
- `cargo check -p tsz-checker -p tsz-solver` - passed.
- `./scripts/conformance/conformance.sh run --filter "paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized" --verbose` - passed, 1/1.
- `./scripts/conformance/conformance.sh run --max 200` - passed, 200/200.
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep -E "FINAL RESULTS|Fingerprint-only|Known failures|Crashed|Timeout|passed"` - passed, 12,438/12,582 (98.9%), fingerprint-only 100.
