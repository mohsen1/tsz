# server: TS6192 unused-import code fix returns no actions (#4024)

- **Date**: 2026-05-07
- **Branch**: `claude/nice-darwin-CDBFL`
- **PR**: TBD
- **Status**: claim
- **Workstream**: LSP code actions / unused-import quick fix

## Intent

Fix issue #4024 — the `unusedIdentifier` quick fix for TS6192 ("All imports
in import declaration are unused") returns no actions because
`import_removal_target` only handles diagnostics anchored at an
`IMPORT_SPECIFIER` or an identifier under an `IMPORT_CLAUSE`. TS6192
diagnostics are anchored at the start of the entire `IMPORT_DECLARATION`,
so the walk never matches and the server sends an empty fix list. Add an
`ImportRemoval::All` branch that walks up to the enclosing
`IMPORT_DECLARATION` and emits a "Remove import from '<module>'" deletion
edit matching tsserver's behavior.

## Files Touched

- `crates/tsz-lsp/src/code_actions/code_action_imports.rs`
- `crates/tsz-lsp/tests/code_actions_tests.rs` (new test)

## Verification

- `cargo nextest run -p tsz-lsp`
