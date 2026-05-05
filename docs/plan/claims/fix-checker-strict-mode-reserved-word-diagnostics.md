# fix(checker): align strict-mode reserved word diagnostics

- **Date**: 2026-05-05
- **Branch**: `fix/checker-strict-mode-reserved-word-diagnostics`
- **PR**: #2927
- **Status**: ready
- **Workstream**: 1 (Conformance)

## Intent

Fix the conformance gap in `TypeScript/tests/cases/compiler/strictModeReservedWord.ts`.
`tsz` currently misses duplicate identifier diagnostics for recovered reserved-word
declarations, skips the class expression name diagnostic for `class package`, and
does not report TS2507 when `extends public` resolves to the local `number` variable.

Root cause: function-body top-level blocks were treated like nested blocks during
binder hoisting, so recovered function declarations split away from same-scope vars;
the var/let duplicate fallback then assumed duplicate checking would handle the
function-body block; and class-expression checking skipped the class-name and
non-constructable heritage diagnostics that class declarations already ran.

## Files Touched

- `crates/tsz-binder/src/nodes/binding.rs`
- `crates/tsz-binder/src/state/tests.rs`
- `crates/tsz-checker/src/state/state_checking/class.rs`
- `crates/tsz-checker/src/state/state_checking/heritage.rs`
- `crates/tsz-checker/src/state/state_checking/strict_names.rs`
- `crates/tsz-checker/src/state/variable_checking/variable_helpers/core.rs`
- `crates/tsz-checker/tests/strict_mode_reserved_word_in_qualified_type_tests.rs`

## Verification

- `cargo fmt --all --check` (pass)
- `cargo check --package tsz-binder --package tsz-checker --package tsz-solver` (pass)
- `cargo nextest run --package tsz-binder --lib` (329 passed)
- `cargo nextest run --package tsz-checker --test strict_mode_reserved_word_in_qualified_type_tests` (5 passed)
- `./scripts/conformance/conformance.sh run --filter "strictModeReservedWord" --verbose` (6/6 passed)
- `./scripts/conformance/conformance.sh run --max 200` (200/200 passed)
