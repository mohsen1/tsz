# chore(checker): simplify heritage parent traversal check

- **Date**: 2026-05-12
- **Branch**: `codex/cleanup-checker-heritage-presence-20260512`
- **PR**: #5671
- **Status**: ready
- **Workstream**: DRY cleanup

## Intent

Simplify the checker heritage-support parent traversal sentinel check from
`!current.is_none()` to `current.is_some()`. This is a behavior-preserving
readability cleanup in the helper that collects enclosing interface type
parameter names.

## Files Touched

- `crates/tsz-checker/src/state/state_checking/heritage_support.rs`
- This claim file

## Verification

- `cargo fmt --check` (passed)
- `cargo nextest run -p tsz-checker heritage_support` (0 tests matched; no
  coverage signal)
- `cargo nextest run -p tsz-checker` in this worktree: 7456 passed, 26 failed,
  30 skipped. The failing set matches a clean `origin/main` run.
- Clean `origin/main` comparison in `/private/tmp/tsz-verify-main-checker-20260512`:
  `cargo nextest run -p tsz-checker` also produced 7456 passed, 26 failed,
  30 skipped with the same failure families.
- Relevant heritage traversal tests in the full run passed, including
  `type_arg_count_mismatch_tests::unresolved_heritage_extends_walks_type_arguments`,
  `type_arg_count_mismatch_tests::unresolved_heritage_implements_walks_type_arguments`,
  and `type_arg_count_mismatch_tests::unresolved_heritage_extends_and_implements_walk_type_arguments`.
