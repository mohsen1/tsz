# fix(checker): finish mixin access modifier fingerprints

- **Date**: 2026-05-12
- **Branch**: `fix/mixin-access-modifiers-followup-20260512`
- **PR**: #5756
- **Status**: WIP
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Close the remaining `mixinAccessModifiers.ts` conformance fingerprint drift after the earlier direct intersection-access slice. This follow-up will target the smallest remaining checker/solver path needed to remove the known XFAIL without changing unrelated access-control semantics.

## Files Touched

- `crates/tsz-checker/src/state/state_checking_members/member_declaration_checks.rs`

## Verification

- `cargo fmt --all`
- `./scripts/conformance/conformance.sh run --filter "mixinAccessModifiers" --verbose`

Current delta: mixin-derived instance protected member diagnostics for `p` now match the expected TS2445 fingerprints for the targeted case. Remaining blockers are static protected `s` access fingerprints, generic protected/private method access through `never`, and the TS2509 base constructor return type fingerprints.
