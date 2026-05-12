# test(audit): require JS constructor test libs and retire zod lib false positive

- **Date**: 2026-05-12
- **Branch**: `codex/isolated-20260512-182745`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Address important missed-review test-harness concerns around implicit lib
assumptions.

## Changes

- review comments left on #5830: `check_js_with_es5_and_dom_lib` now fails fast
  if ES5+DOM fixtures are not both loaded, instead of silently running with no
  libs and producing potentially meaningless results.
- review comments left on #5081: the zod path-default regression coverage now
  includes both a no-lib local-utility variant and an explicit lib-backed
  `Partial<Omit<...>>` variant (`load_default_lib_files` asserted non-empty),
  so the original “missing-lib utility alias” concern is no longer outstanding.

## Files Touched

- `crates/tsz-checker/tests/js_constructor_property_tests.rs`
- `docs/plan/claims/codex-review-audit-batch11-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `cargo test -p tsz-checker --test js_constructor_property_tests checked_js_prototype_optional_parent_method_call_suppresses_ts2531 -- --nocapture`
- `cargo fmt --all --check`
- `python3 scripts/session/audit_missed_review_comments.py --limit 500`
