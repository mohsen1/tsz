# fix(server): reject obsolete getSmartSelectionRange / getSyntacticClassifications / getSemanticClassifications

- **Date**: 2026-05-07
- **Time**: 2026-05-07 00:00:00
- **Branch**: `claude/nice-darwin-CFZg2`
- **PR**: TBD
- **Status**: claim
- **Workstream**: server protocol parity (issue #3833)

## Intent

`tsz-server` accepts three protocol command names that TypeScript 6.0.3 no
longer recognises (`getSmartSelectionRange`, `getSyntacticClassifications`,
`getSemanticClassifications`) and answers them with placeholder bodies
plus `success: true`. tsserver answers all three with
`success: false` and "Unrecognized JSON command: ...". This change drops the
dispatcher routes and the now-dead handler methods so these names fall
through to the dispatcher's "unrecognized command" path, matching tsserver.
The supported replacements (`selectionRange`,
`encodedSemanticClassifications-full`) are unchanged.

## Files Touched

- `crates/tsz-cli/src/bin/tsz_server/main.rs` — drop the three dispatcher
  arms.
- `crates/tsz-cli/src/bin/tsz_server/handlers_editing.rs` — remove
  `handle_smart_selection_range`, `handle_syntactic_classifications`,
  `handle_semantic_classifications`, and their private helpers
  (`classify_tokens_syntactically`, `semantic_token_type_name`).
- `crates/tsz-cli/src/bin/tsz_server/tests.rs` — drop the three commands
  from `test_new_commands_are_recognized`; add a new test asserting they
  are now rejected with the unrecognized-command shape.
- `docs/plan/claims/claude-nice-darwin-CFZg2.md` — this claim.

## Verification

- `cargo nextest run --package tsz-cli` (full crate test suite passes).
