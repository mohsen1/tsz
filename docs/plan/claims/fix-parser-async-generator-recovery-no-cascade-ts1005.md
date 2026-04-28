# fix(parser): suppress redundant TS1005 cascade in `async * <name>` recovery

- **Date**: 2026-04-28
- **Branch**: `fix/parser-async-generator-recovery-no-cascade-ts1005`
- **PR**: TBD
- **Status**: claim
- **Workstream**: 1 (Diagnostic conformance — parser error recovery)

## Intent

`recover_from_missing_method_open_paren` in the parser greedily consumes the rest of a malformed object/class member after a missing `(` (e.g. `async * get x() { ... }` or `async * x: 1;`) — but didn't tell the caller it had already eaten the body block or terminator. The caller then ran its own body-lookup, found no `{`, and emitted a redundant TS1005 `'{' expected.` past the actual member end (often at the outer object-literal closing brace, or EOF in multi-file conformance fixtures).

Fix: change `recover_from_missing_method_open_paren` to return `bool` — `true` when it consumed either a `{ ... }` body block or a `;` / `,` member terminator. Both call sites (`parse_object_method_after_name_with_optional` and the class-member parser) thread the flag through and skip the body lookup when set, suppressing the cascade.

## Files Touched

- `crates/tsz-parser/src/parser/state.rs` — `recover_from_missing_method_open_paren` returns `bool` (true on body or terminator consumption).
- `crates/tsz-parser/src/parser/state_expressions_literals.rs` — `parse_object_method_after_name_with_optional` skips body lookup when recovery consumed it.
- `crates/tsz-parser/src/parser/state_statements_class_members.rs` — same threading at the non-asterisk class-member missing-`(` recovery.
- `crates/tsz-parser/tests/state_expression_tests.rs` — two new lock tests for `async * get x()` and `async * x: 1;`.

## Verification

- `cargo nextest run -p tsz-parser` — 700/700 pass (was 698; +2 lock tests).
- `./scripts/conformance/conformance.sh run --filter "parser.asyncGenerators.objectLiteralMethods"` — moves from fingerprint-only failure (3 extra TS1005 at EOF) to PASS.
- Targeted run on `asyncGenerator` filter: 21/21 PASS (no regression).
