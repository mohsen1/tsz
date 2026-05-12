# fix(audit): preserve computed `__unique_*` string keys in mapped/keyof flows

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch6-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close missed important review comments left on #5003: symbol-key detection and mapped-key extraction treated any `"__unique_*"` text as a synthetic unique symbol, which could misclassify user-authored computed string keys and leak unique-symbol semantics into `keyof`/mapped-type flows.

## Files Touched

- `crates/tsz-checker/src/types/queries/core.rs`
- `crates/tsz-checker/src/types/type_node.rs`
- `crates/tsz-checker/tests/ts2344_keyof_bare_tparam_defer_tests.rs`
- `crates/tsz-solver/src/evaluation/evaluate_rules/keyof.rs`
- `crates/tsz-solver/src/type_queries/data/signatures_and_advanced.rs`
- `crates/tsz-solver/src/type_queries/mapped.rs`

## Verification

- `cargo test -p tsz-checker --test ts2344_keyof_bare_tparam_defer_tests -- --nocapture`
- `cargo check -p tsz-solver -p tsz-checker`
- `cargo fmt --all --check`
