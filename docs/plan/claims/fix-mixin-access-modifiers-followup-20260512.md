# fix(checker): finish mixin access modifier fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/mixin-access-modifiers-followup-20260512`
- **PR**: #5756
- **Status**: Complete
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Close the remaining `mixinAccessModifiers.ts` conformance fingerprint drift after the earlier direct intersection-access slice. This follow-up removes the known XFAIL by recovering mixin-derived instance/static accessibility, generic intersection access diagnostics, and TS2509 reduced-base reporting.

## Files Touched

- `crates/tsz-checker/src/checkers/property_checker.rs`
- `crates/tsz-checker/src/state/state_checking/class.rs`
- `crates/tsz-checker/src/state/state_checking_members/member_declaration_checks.rs`
- `crates/tsz-checker/src/state/state_checking_members/mixin_member_access.rs`
- `crates/tsz-checker/src/types/property_access_type/helpers.rs`
- `crates/tsz-checker/src/types/property_access_type/resolve.rs`
- `crates/conformance/src/runner.rs`

## Verification

- `cargo fmt`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target cargo check -p tsz-checker`
- `CARGO_TARGET_DIR=/Users/mohsen/code/tsz/.target scripts/safe-run.sh --limit 75% -- ./scripts/conformance/conformance.sh run --test-dir /Users/mohsen/code/tsz/TypeScript/tests/cases --filter "mixinAccessModifiers" --verbose`

Result: `mixinAccessModifiers.ts` passes as a normal conformance test, so its production suppression entry was removed.
