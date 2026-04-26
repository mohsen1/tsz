# chore(parser/tests): unit tests for parser/node_modifiers.rs

- **Date**: 2026-04-26
- **Branch**: `chore/parser-tests-node-modifiers`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 8 (DRY/test coverage)

## Intent

Add a dedicated unit-test module for `crates/tsz-parser/src/parser/node_modifiers.rs`,
which exposes `NodeArena::has_modifier`, `has_modifier_ref`, `find_modifier`,
`is_static`, `is_declare`, `is_declare_ref`, and `get_visibility_from_modifiers`.
These helpers are the single source of truth for modifier-list queries used by
the binder, checker, emitter, and lowering crates, but currently have only
incidental coverage from a couple of `state_declaration_tests` calls. Direct
unit tests lock down `Public` default visibility, missing-list handling,
keyword detection, and the `_ref` parity variants.

## Files Touched

- `crates/tsz-parser/tests/node_modifiers_tests.rs` (NEW, ~250 LOC additive)
- `crates/tsz-parser/src/parser/mod.rs` (+4 lines: `#[cfg(test)] #[path]` wiring)

## Verification

- `cargo nextest run -p tsz-parser`
