# fix(checker): nested same-wrapper TS2322 suppression too broad

- **Date**: 2026-04-29
- **Branch**: `claude/exciting-keller-tmbgU`
- **PR**: TBD
- **Status**: ready
- **Workstream**: TS2322 conformance

## Intent

`is_nested_same_wrapper_application_assignment` and its message-level
counterpart `is_nested_same_wrapper_assignability_message` were incorrectly
suppressing TS2322 for `Wrapper<Wrapper<A>>` vs `Wrapper<Wrapper<B>>` when A ≠ B.
Both heuristics only checked that the SOURCE argument was a nested wrapper but not
that the TARGET argument was NOT also the same wrapper. The fix adds the missing
guard: only suppress when the target's argument is NOT also the same generic head,
preserving the original PromiseLike coinductive-cycle intent while correctly
reporting type errors for structural generic types like `Box<Box<number>>` vs
`Box<Box<string>>`.

## Files Touched

- `crates/tsz-checker/src/assignability/assignability_checker.rs` (~15 LOC)
- `crates/tsz-checker/src/state/state_checking/source_file.rs` (~10 LOC)
- `crates/tsz-checker/src/tests/dispatch_tests.rs` (+41 LOC, 2 new tests)

## Verification

- `cargo test -p tsz-checker --lib ts2322_nested_generic_alias_two_levels` → ok
- `cargo test -p tsz-checker --lib ts2322_nested_fn_alias_four_levels` → ok
- `scripts/safe-run.sh cargo test -p tsz-checker --lib` → 2999 passed, 2 failed (pre-existing)
- `conformance nestedCallbackErrorNotFlattened` → upgraded from missing-code to fingerprint-only
