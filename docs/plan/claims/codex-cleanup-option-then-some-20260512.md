# [WIP] chore(checker): simplify option filters

- **Date**: 2026-05-11
- **Branch**: `codex/cleanup-option-then-some-20260512`
- **PR**: TBD
- **Status**: claim
- **Workstream**: DRY cleanup

## Intent

Replace a small cluster of verbose option predicates like
`!value.is_none()` and `(!value.is_none()).then_some(value)` with idiomatic
`is_some()` or direct option combinators. This is a behavior-preserving cleanup
limited to checker helper code.

## Files Touched

- `crates/tsz-checker/src/types/computation/large_tuple.rs`
- `crates/tsz-checker/src/types/property_access_type/helpers.rs`
- `docs/plan/claims/codex-cleanup-option-then-some-20260512.md`

## Verification

- Planned: `cargo fmt --check`
- Planned: targeted checker nextest filters for tuple/property access helpers
