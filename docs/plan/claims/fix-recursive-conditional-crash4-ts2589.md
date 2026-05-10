# fix(checker): restore TS2589 for computed recursive conditional args

- **Date**: 2026-05-10
- **Branch**: `fix/recursive-conditional-crash4-ts2589-2026-05-10`
- **PR**: #4901
- **Status**: ready for review
- **Workstream**: diagnostic-conformance (Tier 2 count drift)

## Intent

`recursiveConditionalCrash4.ts` was missing one TS2589 diagnostic at the
recursive `LengthDown` alias. The definition-time recursive-conditional
guard skipped recursive aliases whenever every type argument merely
contained a deferred type parameter. That was too broad for computed
arguments such as `StrIter.Prev<It>`, where the argument is not a bare
passthrough and tsc still reports excessive instantiation.

## Files Touched

- `crates/tsz-checker/src/types/type_checking/type_alias_checking.rs`
  (tighten deferred-passthrough detection for TS2589 probing)

## Verification

- `cargo test -p tsz-checker --lib ts2589_tests::recursive_conditional_type_alias_anchors_ts2589_at_last_self_reference -- --exact`
- `cargo test -p tsz-checker --lib ts2589_tests::bounded_recursive_alias_with_indexed_type_parameter_arg_no_ts2589 -- --exact`
- `./scripts/conformance/conformance.sh run --filter recursiveConditionalCrash4 --verbose --workers 1`
