# fix(checker): align signature combining rest parameter fingerprints

- **Date**: 2026-05-06
- **Branch**: `fix/checker-signature-combining-rest-parameters-fingerprints`
- **PR**: #3642
- **Status**: ready
- **Workstream**: 1 (conformance)

## Intent

Claiming `TypeScript/tests/cases/compiler/signatureCombiningRestParameters5.ts`.

Current `origin/main` reports the expected TS2345 code, but the diagnostic
fingerprints differ for rest-parameter signature combination. The first error
prints `true[]` where TSC prints `boolean[]`, and the second callback error is
missing the expected `number[]` argument diagnostic.

## Verification

- `CARGO_TARGET_DIR=.target/nextest-local cargo nextest run -p tsz-checker --lib ts2345_array_literal_call_argument_display_widens_boolean_literal_element union_rest_tuple_callback_reports_nested_array_argument_mismatch`
- `./scripts/conformance/conformance.sh run --filter "signatureCombiningRestParameters5" --verbose`
- `./scripts/conformance/conformance.sh run --max 200`
- `git diff --check`
- `scripts/architecture-check.sh --quick`
- `cargo clippy -p tsz-checker --lib -- -D warnings`
- `cargo clippy -p tsz-lowering --lib -- -D warnings`
