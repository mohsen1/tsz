# fix(checker): bind call-signature this returns (#6383)

- **Date**: 2026-05-13
- **Branch**: `fix-this-call-signature-return-6383-20260513`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance / public false-positive fixes

## Intent

Fix #6383, where a callable interface with a call signature returning `this` produces a TS2741 false positive when the call result is assigned back to the same interface. Direct calls like `chain()` have no property-access receiver, so checker call-result finalization now binds a surviving `ThisType` return to the callable callee type itself while leaving receiver-based `obj.method()` substitution unchanged.

## Files Touched

- `crates/tsz-checker/src/types/computation/call_result.rs`
- `crates/tsz-checker/tests/ts2322_tests.rs`
- `docs/plan/claims/fix-this-call-signature-return-6383-20260513.md`

## Verification

- `cargo test -p tsz-checker --test ts2322_tests callable_interface_call_signature_returning_this_preserves_members -- --nocapture` passed
- `cargo test -p tsz-checker --test ts2322_tests -- --nocapture` passed (199 passed)
- `cargo fmt --all -- --check` passed
