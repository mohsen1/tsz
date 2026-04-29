# [WIP] fix(checker): align narrowed-never TS2322 assignment display

- **Date**: 2026-04-28
- **Branch**: `fix/checker-narrowing-union-never-assignment-display`
- **PR**: #1703
- **Status**: ready
- **Workstream**: 1 (Diagnostic conformance and fingerprints)

## Intent

Fix the fingerprint-only TS2322 mismatch in
`TypeScript/tests/cases/compiler/narrowingUnionToNeverAssigment.ts`, selected by
`scripts/session/quick-pick.sh`. The initial scope is the assignment diagnostic
display path for union narrowing to `never`; implementation will follow the
shared checker/solver boundary rules in `.claude/CLAUDE.md`.

## Files Touched

- `docs/plan/claims/fix-checker-narrowing-union-never-assignment-display.md`
- `crates/tsz-checker/src/error_reporter/core/diagnostic_source.rs` (~55 LOC)
- `crates/tsz-checker/tests/ts2322_tests.rs` (~30 LOC)

## Verification

- `cargo check --package tsz-checker` (pass)
- `cargo check --package tsz-solver` (pass)
- `cargo build --profile dist-fast --bin tsz` (pass)
- `cargo nextest run -p tsz-checker test_ts2322_narrowed_string_literal_residual_union_to_never_display` (2 tests pass)
- `cargo nextest run --package tsz-checker --lib` (2950 pass, 2 pre-existing failures: LOC guard on unrelated files; `enum_nominality_tests::test_number_literal_to_numeric_enum_type`)
- `./scripts/conformance/conformance.sh run --filter "narrowingUnionToNeverAssigment" --verbose` (1/1 pass)
- `./scripts/conformance/conformance.sh run --max 200` (199/200 pass; `aliasDoesNotDuplicateSignatures.ts` reproduced with implementation diff removed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 12161/12582 passed (96.7%)`)
