# chore(audit): retire additional stale high-priority review candidates

- **Date**: 2026-05-12
- **Branch**: `codex/isolated-20260512-182745`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close additional stale important review comments that are already addressed on
`main`, so the 500-PR missed-comment queue reflects unresolved work.

## Evidence Snapshot

- review comments left on #4973: cross-file declaration lookup in
  `declared_type_of_identifier_argument` no longer relies only on
  `ctx.arena`; it resolves stable declarations across arenas/files.
- review comments left on #4988: mapped-type conformance helper uses checked
  span slicing (`source.get(start..end)`) with explicit panic context.
- review comments left on #4989: cross-file value-declaration typing uses
  file-aware fallback paths instead of local-arena-only lookup.
- review comments left on #4992: parenthesized explicit-any alias tests now
  assert TS2344 suppression alongside TS2315.
- review comments left on #5086: persistent-scope id allocation is guarded by
  `next_persistent_scope_id(...)` and warns/returns instead of panicking.
- review comments left on #5096: union-display-sensitive TS2322 assertions are
  substring-based and order-agnostic.
- review comments left on #5108: tuple element contextual mapping is
  rest/suffix aware (`target_tuple_element_for_literal_position`).
- review comments left on #5114: TS2322 fingerprint assertions are substring-
  based, avoiding brittle full-message equality.
- review comments left on #5640: collapsed enum-union formatting returns joined
  output when enum-member rendering succeeds.
- review comments left on #5660: JSX mixed-component TS2786 test asserts
  diagnostic anchor at the tag start.
- review comments left on #5666: property-access dot mapping skips comment
  contents when finding `.` token positions.
- review comments left on #5691: class-property semicolon-only typedef
  resolution has dedicated regression coverage.
- review comments left on #5693: JSDoc function-signature parsing uses
  top-level splitting rather than naive `split(',')`.
- review comments left on #5694: generic name-like JSDoc detection rejects
  legacy dot-generic bases ending with `.`.

## Files Touched

- `docs/plan/claims/codex-review-audit-batch10-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `python3 scripts/session/audit_missed_review_comments.py --limit 500`
- Spot checks in the files listed in **Evidence Snapshot**.
