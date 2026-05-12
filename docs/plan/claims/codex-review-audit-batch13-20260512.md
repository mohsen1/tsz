# fix(dts): avoid nested-union false positives in accessor nullish merge

- **Date**: 2026-05-12
- **Branch**: `codex/isolated-20260512-182745`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close important review comments left on #5901 around declaration emit nullish
union detection.

The old `type_text_union_contains` used `split('|')`, so nested unions inside
arrays/generics/parenthesized types could be misread as top-level nullish
branches and incorrectly widen accessor setter signatures.

## Changes

- review comments left on #5901:
  - replaced naive union splitting with top-level union branch parsing that
    tracks delimiter depth and quoted segments.
  - nullish matching now checks normalized top-level branches only.
- added regression coverage proving `(string | null)[]` backing fields do not
  append top-level `| null`/`| undefined` to JS accessor setter declarations.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/core/emit_members.rs`
- `crates/tsz-emitter/src/declaration_emitter/tests/simple_declarations.rs`
- `docs/plan/claims/codex-review-audit-batch13-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `cargo test -p tsz-emitter js_accessor_backing -- --nocapture`
- `cargo fmt --all --check`
- `python3 scripts/session/audit_missed_review_comments.py --limit 500`
