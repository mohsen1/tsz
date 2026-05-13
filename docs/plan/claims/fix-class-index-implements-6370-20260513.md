# fix(checker): suppress duplicate TS2420 for class index implements (#6370)

- **Date**: 2026-05-13
- **Branch**: `fix-class-index-implements-6370-20260513`
- **PR**: #6396
- **Status**: ready
- **Workstream**: conformance / public false-positive fixes

## Intent

Fix #6370, where a class implementing an index-signature-only interface emits a duplicate TS2420 even though the class declares a compatible index signature. The whole-type implements fallback now suppresses the duplicate class-level diagnostic when the class index signature satisfies the interface index signature; the expected member/index TS2411 diagnostic for incompatible named members remains intact.

## Files Touched

- `crates/tsz-checker/src/classes/class_implements_checker/core.rs`
- `crates/tsz-checker/tests/class_index_signature_compat_tests.rs`
- `docs/plan/claims/fix-class-index-implements-6370-20260513.md`

## Verification

- `cargo test -p tsz-checker class_implements_matching_index_signature_does_not_emit_duplicate_ts2420 -- --nocapture` passed
- `cargo fmt --all -- --check` passed
