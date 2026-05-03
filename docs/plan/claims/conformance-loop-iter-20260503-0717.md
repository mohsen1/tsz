# fix(parser): emit TS1275/TS1276 for `accessor` modifier; classify TS1029/1030/1243 as parser grammar codes

- **Date**: 2026-05-03
- **Branch**: `conformance/loop-iter-20260503-0717`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Conformance)

## Intent

`autoAccessorDisallowedModifiers.ts` was missing TS1070 / TS1275 / TS1276 because:

1. tsz never emitted TS1275 (`'accessor' modifier can only appear on a property declaration`) or TS1276 (`An 'accessor' property cannot be declared optional`) anywhere; both codes existed only in `data.rs`.
2. The driver's `is_parser_grammar_code` filter classified TS1029/1030/1243 as "non-grammar parse errors", which then suppressed sibling grammar codes (e.g. TS1070) via the `hasParseDiagnostics`-style gate. tsc emits all of these via `grammarErrorOnNode` from the checker, so it does not self-suppress.

This PR (a) adds TS1275 emission at the `accessor` modifier-keyword position for class members that turn out to be a constructor / method / getter / setter, plus the top-level `accessor <decl>` case; (b) adds TS1276 emission at the `?` token of an optional auto-accessor property; (c) adds TS1029/1030/1243/1275/1276 to `is_parser_grammar_code` so they coexist with siblings instead of suppressing them.

## Files Touched

- `crates/tsz-parser/src/parser/state_statements_class_members.rs` (~50 LOC)
- `crates/tsz-parser/src/parser/state_statements.rs` (~5 LOC)
- `crates/tsz-cli/src/driver/check_utils.rs` (+5 codes in `is_parser_grammar_code`)
- `crates/tsz-parser/tests/modifier_ordering_tests.rs` (+11 tests)

## Verification

- `cargo nextest run -p tsz-parser modifier_ordering_tests` (25 tests pass, 11 new)
- `cargo nextest run -p tsz-parser -p tsz-cli --lib` (1494 tests pass)
- `./scripts/conformance/conformance.sh run --filter "autoAccessorDisallowedModifiers" --verbose` → 1/1 passed
- Full conformance baseline confirmed no regressions (see PR description for delta).
