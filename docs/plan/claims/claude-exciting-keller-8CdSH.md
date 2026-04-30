# fix(checker): aliased discriminant switch narrowing + loose-equality discriminant narrowing

- **Date**: 2026-04-30
- **Branch**: `claude/exciting-keller-8CdSH`
- **PR**: TBD
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance)

## Intent

Two control-flow narrowing gaps caused 3 spurious TS2339 errors on `controlFlowAliasing.ts`:

1. **Switch narrowing via destructured discriminant** (`f33`): `switch_can_affect_reference`
   did not detect that `switch(kind)` where `const { kind } = obj` could affect `obj`.
   Added `is_aliased_discriminant_switch_expr` helper that detects both simple-alias
   (`const kind = obj.kind`) and destructuring-alias (`const { kind } = obj`) forms.

2. **Loose-equality discriminant narrowing** (`f40`): `discriminant_comparison` and
   `literal_comparison` were gated on `is_strict` (`===` only), so `const isFoo = kind == 'foo'`
   did not trigger discriminant narrowing. Extended gate to `is_strict || is_equals` since
   null/undefined loose equality is handled by the prior `nullish_comparison` block.

## Files Touched

- `crates/tsz-checker/src/flow/control_flow/narrowing.rs` — added `is_aliased_discriminant_switch_expr`
- `crates/tsz-checker/src/flow/control_flow/core.rs` — `switch_can_affect_reference` calls new helper
- `crates/tsz-checker/src/flow/control_flow/condition_narrowing.rs` — `is_strict` → `is_strict || is_equals`
- `crates/tsz-checker/tests/control_flow_type_guard_tests.rs` — 2 new regression tests

## Verification

- `controlFlowAliasing.ts` conformance: 3/3 passed (100%, was fingerprint-only failure)
- `cargo test --package tsz-checker --lib`: 3025 passed, 0 failed
- `cargo test --package tsz-checker --test control_flow_type_guard_tests`: 22 passed, 0 failed
- `conformance.sh run --filter controlFlow`: 86/94 passed (91.5%)
