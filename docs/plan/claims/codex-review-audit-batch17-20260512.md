# fix(audit): preserve contextual initializer cache through jsdoc raw-new relation check

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch17-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close the important unresolved review comment on #5690 about cache integrity in
`check_variable_declaration_with_request` when the JSDoc `@type` + `new`
initializer relation path performs an additional raw initializer re-check.

## Changes

- review comments left on #5690:
  - snapshot and restore the initializer entry in `ctx.node_types` around the
    `TypingRequest::NONE` re-check inside `jsdoc_new_expression_relation`.
  - keep the raw relation check behavior for assignability parity, while
    preventing the raw pass from permanently overwriting the context-seeded
    initializer cache entry.
- review comments left on #4967:
  - verified current `generic_call/resolve.rs` no longer stores a global
    `display_alias` on fallback constraints; it now records fallback display
    types in call-scoped `constraint_fallback_display_types`.
  - verified the previously flagged clone-heavy patterns are no longer present:
    no `display_subst = final_subst.clone()` display override path and no
    `un_widened.clone()` union construction in this fallback branch.

## Files Touched

- `crates/tsz-checker/src/state/variable_checking/core.rs`
- `crates/tsz-solver/src/operations/generic_call/resolve.rs` (verified current behavior; no edit needed)
- `docs/plan/claims/codex-review-audit-batch17-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `cargo test -p tsz-checker --test jsdoc_cross_file_typedef_tests jsdoc_type_assignment_new_expression_reports_subclass_mismatch -- --nocapture`
- `cargo test -p tsz-checker --test jsdoc_cross_file_typedef_tests jsdoc_type_assignment_binds_interface_this_to_source_instance -- --nocapture`
- `cargo test -p tsz-checker --test generic_call_primitive_widening_display_tests -- --nocapture`
- `cargo fmt --all --check`
- `python3 scripts/session/audit_missed_review_comments.py --limit 500`
