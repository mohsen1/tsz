# chore(audit): close stale high-priority review-thread candidates

- **Date**: 2026-05-12
- **Branch**: `codex/isolated-20260512-182745`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close stale high-priority review comments left on #4952, #4958, #5002, #5051,
#5057, #5067, #5089, #5102, #5658, and #5662 after verifying their
underlying concerns are already addressed on `main`.

This keeps the 500-PR missed-comment audit focused on true outstanding
follow-ups instead of repeatedly surfacing already-landed fixes.

## Evidence Snapshot

- review comments left on #4952: the claim + run summary now match the
  measured scope (`large-ts-repo` deferred), and the committed raw diag JSON
  has portable command-lines plus populated `fixture.*` provenance.
- review comments left on #4958: class-expression recovery clears
  `pending_const_binding_name_colon` on non-binding tokens.
- review comments left on #5002: orphan `case = ...` recovery now stops at
  `}` or line breaks (ASI-safe), not only explicit semicolons.
- review comments left on #5051: variance conformance test asserts both the
  TS2322 count and total diagnostics count (no extra non-TS2322 cascades).
- review comments left on #5057: hoisted return-context type parameters are
  explicitly sorted (`sort_type_params_by_name`) for deterministic ordering.
- review comments left on #5067: tuple source display resolves literal
  positions through rest/suffix-aware tuple slot mapping.
- review comments left on #5089: parser `usize -> u32` / node-flag `u32 -> u16`
  helpers are non-panicking and clamp/truncate with warnings.
- review comments left on #5102: parser recovery tests cover nested
  conditional branches and nested semicolons in skipped branches.
- review comments left on #5658: static-class-expression detection unwraps
  `NON_NULL_EXPRESSION` via unary accessors.
- review comments left on #5662: System `react-jsxdev` emit refreshes
  `_jsxFileName` per source file and has a stale-value regression test.

## Files Touched

- `docs/plan/claims/codex-review-audit-batch8-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `python3 scripts/session/audit_missed_review_comments.py --limit 500`
- Spot checks in affected files listed in **Evidence Snapshot**.
