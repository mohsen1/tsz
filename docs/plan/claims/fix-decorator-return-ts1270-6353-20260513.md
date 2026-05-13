# fix(checker): validate decorator return types with TS1270

- **Date**: 2026-05-13
- **Branch**: `fix-decorator-return-ts1270-6353-20260513`
- **PR**: #6365
- **Status**: ready
- **Workstream**: conformance diagnostics

## Intent

Fix issue #6353 by validating legacy experimental class decorator return types and emitting TS1270 when a decorator returns a value incompatible with `void | typeof Class`.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/class.rs` — validate class decorator call return types against `void | typeof Class` and emit TS1270.
- `crates/tsz-checker/tests/ts1238_tests.rs` — cover the issue repro and compatible `void`/replacement class returns.

## Verification

- `cargo fmt --all && cargo test -p tsz-checker --test ts1238_tests ts1270 -- --nocapture && cargo fmt --all -- --check` — passed, 2 TS1270 tests passed.
