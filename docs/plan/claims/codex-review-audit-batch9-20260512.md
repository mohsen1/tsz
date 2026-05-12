# test(audit): harden JSX and generic-call diagnostic assertions

- **Date**: 2026-05-12
- **Branch**: `codex/isolated-20260512-182745`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close important missed review comments left on #4994, #5743, and #5811 by
making the affected regression tests deterministic and semantically aligned
with the behavior they claim to verify.

## Changes

- review comments left on #4994: replace order-sensitive `find`-based TS2345
  checks in generic-call inference tests with anchor-specific assertions using
  raw diagnostics (`code + start`) for the intended argument sites.
- review comments left on #5743: strengthen JSX union tests to explicitly
  exercise class-component validation paths with `JSX.ElementClass` and
  `JSX.ElementAttributesProperty`, plus explicit class `props`/constructor
  shape in both invalid and valid-union cases.
- review comments left on #5811: tighten the `ComponentType` union regression
  to assert a single TS2786 for `<Bad ...>` and verify the diagnostic anchor
  start location.

## Files Touched

- `crates/tsz-checker/src/checkers/jsx/tests.rs`
- `crates/tsz-checker/tests/generic_call_inference_tests.rs`
- `docs/plan/claims/codex-review-audit-batch9-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `cargo test -p tsz-checker --lib jsx_union_component_ -- --nocapture`
- `cargo test -p tsz-checker --lib jsx_user_named_component_type_alias_union_still_checks_returns -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_inference_tests generic_argument_mismatch -- --nocapture`
- `cargo fmt --all --check`
