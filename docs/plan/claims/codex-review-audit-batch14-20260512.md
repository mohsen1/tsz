# test(audit): cover cjs typedef export-equals function path and retire resolved system decorator threads

- **Date**: 2026-05-12
- **Branch**: `codex/review-audit-batch14-20260512`
- **PR**: TBD
- **Status**: ready
- **Workstream**: review audit follow-ups

## Intent

Close important review comments left on #5753 and #5717 from the missed-review
audit queue.

## Changes

- review comments left on #5753:
  - added a focused declaration-emitter regression test for JS
    `const fn = (...) => ...; module.exports = fn;` with a leading multiline
    JSDoc `@typedef`.
  - the test now explicitly covers the export-equals function-variable path and
    asserts ordering/presence of:
    - `export = send;`
    - `declare function send(...)`
    - `declare namespace send { export { ResolveRejectMap }; }`
    - `type ResolveRejectMap = ...`
- review comments left on #5717:
  - verified current mainline code already addressed the flagged System legacy
    decorator concerns:
    - helper detection/emission includes `__decorate`, `__param`, and
      `__metadata` based on class/member/parameter usage.
    - legacy decorator export folding uses a temporary writer buffer rather than
      truncate-and-rewrite of the main output.
    - helper-placement test asserts `__decorate` appears after
      `System.register(...` callback `"use strict"` prologue.

## Files Touched

- `crates/tsz-emitter/src/declaration_emitter/tests/simple_declarations.rs`
- `docs/plan/claims/codex-review-audit-batch14-20260512.md`
- `docs/plan/review-comment-audit-latest.json`
- `docs/plan/review-comment-audit-latest.md`

## Verification

- `cargo test -p tsz-emitter test_js_typedef_before_cjs_export_equals_function_variable_path_is_covered -- --nocapture`
- `cargo test -p tsz-emitter system_exported_legacy_decorated_class_exports_decorator_assignment -- --nocapture`
- `cargo test -p tsz-emitter system_nested_legacy_decorated_class_emits_decorate_helper -- --nocapture`
- `cargo test -p tsz-emitter system_legacy_constructor_param_decorators_emit_param_helper -- --nocapture`
- `cargo test -p tsz-emitter system_legacy_decorator_metadata_emits_metadata_helper -- --nocapture`
- `python3 scripts/session/audit_missed_review_comments.py --limit 500`
