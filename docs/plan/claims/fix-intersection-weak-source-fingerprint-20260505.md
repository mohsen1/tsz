# fix(checker): align intersection weak source fingerprints

- **Date**: 2026-05-05
- **Branch**: `fix/intersection-weak-source-fingerprint-20260505`
- **PR**: #2767
- **Status**: ready
- **Workstream**: conformance fingerprint parity

## Intent

Fix the fingerprint-only divergence in
`TypeScript/tests/cases/conformance/types/intersection/intersectionAsWeakTypeSource.ts`.
The target already emits the expected diagnostic codes (`TS2559`, `TS2739`), so
this PR will root-cause the message, type-display, count, or position mismatch
and fix it in the owning layer rather than suppressing diagnostics.

Root cause: assignment-source diagnostics received the generic application
`Brand<T>` and preserved the alias before the existing structural-intersection
formatter could see the evaluated `number & { __brand: T }` body. The fix
detects generic applications that evaluate to primitive-member intersections and
routes them through the structural intersection formatter used for direct
intersection display.

This fresh claim supersedes the stale unbacked
`docs/plan/claims/claude-exciting-keller-raRPb.md` entry for the same
conformance file, which is marked ready but has no live remote branch or PR.

## Files Touched

- `crates/tsz-checker/src/error_reporter/core_formatting.rs` — detect generic
  applications that evaluate to primitive-member intersections before alias
  display wins.
- `crates/tsz-checker/tests/intersection_primitive_member_assignability_tests.rs`
  — TS2739 regression for branded primitive source display.
- `crates/tsz-checker/src/checkers/jsx/props/synthesized_display.rs` and
  sibling module wiring — behavior-preserving split to keep the checker
  file-size architecture guard green.

## Verification

- `cargo fmt --all`
- `cargo check --package tsz-checker`
- `cargo check --package tsz-solver`
- `cargo build --profile dist-fast --bin tsz`
- `cargo nextest run -p tsz-checker branded_primitive_application_source_displays_structural_intersection_in_ts2739`
- `cargo nextest run -p tsz-checker architecture_contract_tests_src::test_checker_file_size_ceiling branded_primitive_application_source_displays_structural_intersection_in_ts2739`
- `cargo nextest run --package tsz-checker --lib` (3337 passed, 10 skipped)
- `cargo nextest run --package tsz-solver --lib` (5623 passed, 9 skipped)
- `./scripts/conformance/conformance.sh run --filter "intersectionAsWeakTypeSource" --verbose` (1/1 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL` (`FINAL RESULTS: 12440/12582 passed (98.9%)`)
