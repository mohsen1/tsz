# fix(parser): preserve modifier-like type parameter names

- **Date**: 2026-04-29
- **Branch**: `fix/parser-convert-keywords-yes`
- **PR**: #1820
- **Status**: ready
- **Workstream**: 1 (Diagnostic Conformance And Fingerprints)

## Intent

This PR targets the random conformance pick `convertKeywordsYes.ts`.
Current TSZ output misses `TS1213` and emits extra `TS1139`, `TS2300`, and
`TS2749` fingerprints compared with `tsc`. The slice will diagnose the root
cause in parser/checker keyword-conversion handling and land the smallest
architecture-aligned fix with an owning Rust regression test.

## Files Touched

- `crates/tsz-parser/src/parser/state_expressions.rs` (type-parameter modifier recovery)
- `crates/tsz-parser/tests/state_type_tests.rs` (parser regression)
- `crates/tsz-checker/Cargo.toml` (integration test target)
- `crates/tsz-checker/tests/convert_keywords_yes_tests.rs` (checker diagnostic regression)
- `crates/tsz-checker/src/error_reporter/core/type_display.rs` (line-count guard cleanup after rebase)
- `crates/tsz-cli/src/reporting/reporter.rs` (current-base clippy cleanup after rebase)
- `docs/plan/claims/fix-parser-convert-keywords-yes.md` (claim)

## Verification

- `cargo check --package tsz-parser`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo nextest run -p tsz-parser parse_modifier_like_type_parameter_names_without_empty_name_recovery`
- `cargo nextest run -p tsz-checker --test convert_keywords_yes_tests`
- `cargo nextest run --package tsz-parser --lib` (717 passed, 1 skipped)
- `cargo nextest run --package tsz-checker --lib` (3007 passed, 10 skipped)
- `cargo nextest run --package tsz-solver --lib` (5551 passed, 9 skipped)
- `./scripts/conformance/conformance.sh run --filter "convertKeywordsYes" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 12251/12582 passed (97.4%)`)
- `cargo fmt --all --check`
- `scripts/ci/github-suite.sh lint` (passes; local GCS cache restore reports non-fatal reauth noise)
