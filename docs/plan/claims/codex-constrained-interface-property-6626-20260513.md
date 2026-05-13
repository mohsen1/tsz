# fix(checker): allow constrained interface property access

- **Date**: 2026-05-13
- **Branch**: `codex/constrained-interface-property-6626-20260513`
- **PR**: #6637
- **Status**: ready
- **Workstream**: conformance / checker property access

## Intent

Fix #6626 so a generic value `T extends Interface<any>` can access properties
declared by the interface without a false TS2339.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/property_access.rs`
- `crates/tsz-checker/src/types/property_access_type/helpers.rs`
- `crates/tsz-checker/tests/ts2339_union_narrow_display_tests.rs`
- `docs/plan/claims/codex-constrained-interface-property-6626-20260513.md`

## Verification

- `cargo test -p tsz-checker --test ts2339_union_narrow_display_tests constrained_interface_type_parameter_property_access_no_ts2339 -- --nocapture`
- `cargo test -p tsz-checker --test ts2339_union_narrow_display_tests -- --nocapture`
- `cargo fmt --all --check`
