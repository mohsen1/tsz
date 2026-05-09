# Lock outer-call inference for generic callbacks (#3768)

- **Date**: 2026-05-09
- **Branch**: `claude/nice-darwin-5oSLe`
- **PR**: TBD
- **Status**: ready
- **Workstream**: conformance / checker generic call inference

## Intent

Issue #3768 reports two repros where tsz emits false TS2345 because a generic
callback argument is not instantiated from the outer generic call inference
context. Both repros currently exit `0` against the latest `main`
(`.target/release/tsz --noEmit ...`) — the underlying fix is already landed.
This claim adds a comprehensive regression test file that locks both repros
plus alternate-name and structural-edge variants in place, so future churn in
`instantiate_generic_function_argument_against_target_params` cannot silently
re-break the issue.

The test file is intentionally distinct from PR #4672's narrower test (which
covers only the two verbatim repros) — this one adds dotted-callback,
alternate type-parameter spellings (per CLAUDE.md §25 anti-hardcoding), and
the zero-rest-args edge case for the second repro shape.

## Files Touched

- `crates/tsz-checker/tests/issue_3768_outer_call_inference_regression_tests.rs` (new, 7 tests)
- `crates/tsz-checker/Cargo.toml` (register the new test target — `autotests = false`)
- `docs/plan/claims/claude-nice-darwin-5oSLe.md` (this file)

## Verification

- `cargo test -p tsz-checker --test issue_3768_outer_call_inference_regression_tests`
  → 7 passed; 0 failed
- `.target/release/tsz --noEmit /tmp/tsz-3768/{map,spread,constraint}.ts` → exit 0 on all repros
- `cargo test -p tsz-binder --lib` (pending in CI)
- `cargo test -p tsz-solver --lib` (pending in CI)
- `cargo test -p tsz-checker --lib` (pending in CI)
