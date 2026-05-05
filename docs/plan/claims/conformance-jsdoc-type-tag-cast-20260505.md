# [WIP] fix(conformance): align JSDoc type tag cast diagnostics

- **Date**: 2026-05-05
- **Branch**: `conformance/jsdoc-type-tag-cast-20260505`
- **PR**: #3038
- **Status**: abandoned
- **Workstream**: 1 (Diagnostic conformance)

## Intent

Fix the picked `jsdocTypeTagCast.ts` conformance mismatch. The current
fingerprint is missing TS1228 and emits an extra TS2403, so the investigation
will identify whether the root cause is JSDoc tag validation, duplicate
declaration classification, or a shared diagnostic anchoring/rendering path.

Abandoned before implementation because the picked target already passes on
the fresh `origin/main` worktree. The picker input was stale for this case.

## Files Touched

- TBD after investigation.

## Verification

- `./scripts/conformance/conformance.sh run --filter "jsdocTypeTagCast" --verbose` (1/1 passed, no fingerprint-only failures)
- focused Rust unit tests in the owning crate
- `cargo build --profile dist-fast --bin tsz`
- `./scripts/conformance/conformance.sh run --max 200`
- `scripts/safe-run.sh ./scripts/conformance/conformance.sh run 2>&1 | grep FINAL`
