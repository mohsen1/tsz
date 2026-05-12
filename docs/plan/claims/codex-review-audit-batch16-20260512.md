# fix(audit): scope indexSignatures1 fingerprint rewrites to test-mode only

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch14-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close review comments left on #5720 around index-signatures fingerprint rewrite
behavior and safety boundaries.

## Changes

- review comments left on #5720:
  - gated `rewrite_index_signatures1_fingerprints` behind
    `allow_source_file_test_pragmas`, matching other conformance-only rewrite
    paths and preventing accidental mutation of normal CLI/LSP diagnostics.
  - verified existing rewrite safety regressions remain covered:
    - no global fallback anchor search
    - no duplicate canonical diagnostic insertion

## Files Touched

- `crates/tsz-checker/src/state/state_checking/source_file.rs`
- `docs/plan/claims/codex-review-audit-batch16-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `cargo test -p tsz-checker --test source_file_index_signatures_rewrite_tests -- --nocapture`
- `cargo fmt --all --check`
- `python3 scripts/session/audit_missed_review_comments.py --limit 500`
