# fix(checker): validate decorator return types with TS1270

- **Date**: 2026-05-13
- **Branch**: `fix-decorator-return-ts1270-6353-20260513`
- **PR**: TBD
- **Status**: claim
- **Workstream**: conformance diagnostics

## Intent

Fix issue #6353 by validating legacy experimental decorator return types and emitting TS1270 when a decorator returns a value incompatible with the decorated target. The initial scope is class decorator return validation for `void | typeof Class`, matching the public conformance blocker; method/accessor/property/parameter return validation will be added if the existing checker plumbing exposes the required target types cleanly in the same PR-sized slice.

## Files Touched

- `crates/tsz-checker/src/state/state_checking_members/decorator_signature_checks.rs` — add TS1270 return-type validation helpers.
- `crates/tsz-checker/src/state/state_checking_members/member_declaration_checks.rs` or class declaration checker dispatch — call the helper for class decorators.
- `crates/tsz-checker/tests/ts1270_decorator_return_tests.rs` or adjacent decorator test — cover the issue repro and a compatible decorator.

## Verification

- Pending implementation.
